use itertools::Itertools;
use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_python_core::ProgramFile;

use super::{ProjectionError, ProjectionTypeBudget, SolutionBudget, SolutionProjection};
use crate::db::tests::{TestDb, setup_db};
use crate::place::global_symbol;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension, PathBound,
    PathBoundSolution, PathBounds, Solution, SolutionPaths, Solutions, TypeVarSolution,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, IntersectionType, KnownClass, Type, TypeVarVariance, UnionType,
};

type Paths<'db> = FxHashSet<Solution<'db>>;

fn create_typevar<'db>(db: &'db TestDb, name: &'static str) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::synthetic(
        db,
        &db.program_environment(),
        Name::new_static(name),
        TypeVarVariance::Invariant,
    )
}

fn known_instance(db: &TestDb, class: KnownClass) -> Type<'_> {
    class.to_instance(db, &db.program_environment())
}

fn exact<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    typevar: BoundTypeVarInstance<'db>,
    ty: Type<'db>,
) -> ConstraintSet<'db, 'c> {
    ConstraintSet::constrain_typevar(db, &db.program_environment(), builder, typevar, ty, ty)
}

fn binary_choice<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    typevar: BoundTypeVarInstance<'db>,
    alternatives: [Type<'db>; 2],
) -> ConstraintSet<'db, 'c> {
    alternatives
        .into_iter()
        .when_any(db, builder, |ty| exact(db, builder, typevar, ty))
}

fn binding<'db>(
    bound_typevar: BoundTypeVarInstance<'db>,
    solution: Type<'db>,
) -> TypeVarSolution<'db> {
    TypeVarSolution {
        bound_typevar,
        solution,
    }
}

fn collect_paths<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    set: ConstraintSet<'db, 'c>,
    typevars: &[BoundTypeVarInstance<'db>],
    budget: SolutionBudget,
) -> Result<SolutionProjection<Paths<'db>>, ProjectionError> {
    let env = db.program_environment();
    set.try_fold_solutions(
        db,
        &env,
        TypeVarSet::from_typevars(db, typevars.iter().copied()),
        budget,
        |_, bound| PathBounds::default_solve(db, &env, builder, bound),
        Paths::default(),
        |mut paths, path, budget| {
            for binding in path {
                budget.charge_type(db, binding.solution)?;
            }
            let mut path = path.to_vec();
            path.sort_by_key(|binding| {
                typevars
                    .iter()
                    .position(|typevar| *typevar == binding.bound_typevar)
            });
            paths.insert(path);
            Ok(paths)
        },
    )
}

#[test]
fn path_limit_is_checked_before_solving() {
    let db = setup_db();
    let db = &db;
    let env = db.program_environment();
    let t = create_typevar(db, "T");
    let u = create_typevar(db, "U");
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let builder = ConstraintSetBuilder::new();
    let set = binary_choice(db, &builder, t, [int, str])
        .and(db, &builder, || binary_choice(db, &builder, u, [int, str]));
    let inferable = TypeVarSet::from_typevars(db, [t, u]);

    for max_paths in [0, 3, 4] {
        let mut selected = 0;
        let mut folded = 0;
        let result = set.try_fold_solutions(
            db,
            &env,
            inferable,
            SolutionBudget {
                paths: max_paths,
                ..SolutionBudget::default()
            },
            |_, bound| {
                selected += 1;
                PathBounds::default_solve(db, &env, &builder, bound)
            },
            0,
            |count, _, _| {
                folded += 1;
                Ok(count + 1)
            },
        );

        if max_paths < 4 {
            assert_eq!(result, Err(ProjectionError::PathBudgetExceeded));
            assert_eq!(selected, 0);
            assert_eq!(folded, 0);
        } else {
            assert_eq!(result, Ok(SolutionProjection::Constrained(4)));
            assert_eq!(selected, 8);
        }
    }
}

#[test]
fn terminal_projections_need_no_paths_or_types() {
    let db = setup_db();
    let db = &db;
    let t = create_typevar(db, "T");
    let builder = ConstraintSetBuilder::new();

    // Terminal answers do not allocate any path or construct any type.
    let terminal_budget = SolutionBudget {
        paths: 0,
        visits: 1,
        type_terms: 0,
    };
    for (set, expected) in [
        (
            ConstraintSet::always(&builder),
            SolutionProjection::Unconstrained,
        ),
        (
            ConstraintSet::never(&builder),
            SolutionProjection::Unsatisfiable,
        ),
    ] {
        assert_eq!(
            collect_paths(db, &builder, set, &[t], terminal_budget),
            Ok(expected)
        );
    }
}

#[test]
fn source_and_interning_order_do_not_change_correlated_projection() {
    let db = setup_db();
    let db = &db;
    let t = create_typevar(db, "T");
    let u = create_typevar(db, "U");
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let bytes = known_instance(db, KnownClass::Bytes);
    let bool = known_instance(db, KnownClass::Bool);
    let atoms = [(t, int), (t, str), (u, bytes), (u, bool)];
    // These alternatives do not admit the crossed pairings of T and U.
    let expected = FxHashSet::from_iter([
        vec![binding(t, int), binding(u, bytes)],
        vec![binding(t, str), binding(u, bool)],
    ]);

    for interning_order in (0..atoms.len()).permutations(atoms.len()) {
        for reverse_source in [false, true] {
            let builder = ConstraintSetBuilder::new();
            for index in &interning_order {
                let (typevar, ty) = atoms[*index];
                exact(db, &builder, typevar, ty);
            }
            let [t_int, t_str, u_bytes, u_bool] =
                atoms.map(|(typevar, ty)| exact(db, &builder, typevar, ty));
            let set = if reverse_source {
                u_bool
                    .and(db, &builder, || t_str)
                    .or(db, &builder, || u_bytes.and(db, &builder, || t_int))
            } else {
                t_int
                    .and(db, &builder, || u_bytes)
                    .or(db, &builder, || t_str.and(db, &builder, || u_bool))
            };

            assert_eq!(
                collect_paths(db, &builder, set, &[t, u], SolutionBudget::default()),
                Ok(SolutionProjection::Constrained(expected.clone())),
                "interning order {interning_order:?}, reverse source {reverse_source}"
            );
            assert_eq!(
                collect_paths(
                    db,
                    &builder,
                    set,
                    &[t, u],
                    SolutionBudget {
                        paths: 1,
                        ..SolutionBudget::default()
                    },
                ),
                Err(ProjectionError::PathBudgetExceeded)
            );
        }
    }
}

#[test]
fn four_independent_binary_arguments_have_sixteen_solutions() {
    let db = setup_db();
    let db = &db;
    let typevars = ["T", "U", "V", "W"].map(|name| create_typevar(db, name));
    let alternatives =
        [[1, 2], [3, 4], [5, 6], [7, 8]].map(|choices| choices.map(Type::int_literal));
    let builder = ConstraintSetBuilder::new();

    // Four arguments that independently admit two specializations produce sixteen whole-call
    // solutions. The limit applies before constructing any of their projected return types.
    let set =
        typevars
            .into_iter()
            .zip(alternatives)
            .when_all(db, &builder, |(typevar, alternatives)| {
                binary_choice(db, &builder, typevar, alternatives)
            });
    let expected = alternatives
        .into_iter()
        .multi_cartesian_product()
        .map(|choices| {
            typevars
                .into_iter()
                .zip(choices)
                .map(|(typevar, ty)| binding(typevar, ty))
                .collect()
        })
        .collect();

    assert_eq!(
        collect_paths(
            db,
            &builder,
            set,
            &typevars,
            SolutionBudget {
                paths: 16,
                ..SolutionBudget::default()
            },
        ),
        Ok(SolutionProjection::Constrained(expected))
    );
    assert_eq!(
        collect_paths(
            db,
            &builder,
            set,
            &typevars,
            SolutionBudget {
                paths: 15,
                ..SolutionBudget::default()
            },
        ),
        Err(ProjectionError::PathBudgetExceeded)
    );
}

#[test]
fn incomplete_solution_discards_the_projection() {
    let db = setup_db();
    let db = &db;
    let env = db.program_environment();
    let t = create_typevar(db, "T");
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let builder = ConstraintSetBuilder::new();
    let inferable = TypeVarSet::from_typevars(db, [t]);
    let budget = SolutionBudget {
        type_terms: 2,
        ..SolutionBudget::default()
    };

    for alternatives in [[int, str], [str, int]] {
        let set = binary_choice(db, &builder, t, alternatives);
        let choose = |_, bound: &PathBound<'_>| {
            if bound.evidence_lower == Some(str) {
                PathBoundSolution::BudgetExceeded {
                    fallback: Some(str),
                }
            } else {
                PathBoundSolution::Solved(int)
            }
        };

        assert_eq!(
            set.solutions_with(db, &env, inferable, budget, choose),
            Ok(Solutions::Constrained(SolutionPaths::BudgetExceeded(
                alternatives.map(|ty| vec![binding(t, ty)]).into()
            )))
        );
        assert_eq!(
            set.try_fold_solutions(db, &env, inferable, budget, choose, 0, |count, _, _| Ok(
                count + 1
            ),),
            Err(ProjectionError::IncompleteSolution)
        );
    }
}

#[test]
fn rejected_exhausted_path_does_not_poison_valid_sibling() {
    let db = setup_db();
    let db = &db;
    let env = db.program_environment();
    let t = create_typevar(db, "T");
    let u = create_typevar(db, "U");
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let bytes = known_instance(db, KnownClass::Bytes);
    let inferable = TypeVarSet::from_typevars(db, [t, u]);
    let budget = SolutionBudget {
        type_terms: 1,
        ..SolutionBudget::default()
    };

    for reverse_bounds in [false, true] {
        for reverse_paths in [false, true] {
            let builder = ConstraintSetBuilder::new();
            let t_str = exact(db, &builder, t, str);
            let u_bytes = exact(db, &builder, u, bytes);
            let rejected = if reverse_bounds {
                u_bytes.and(db, &builder, || t_str)
            } else {
                t_str.and(db, &builder, || u_bytes)
            };
            let valid = exact(db, &builder, t, int);
            let set = if reverse_paths {
                valid.or(db, &builder, || rejected)
            } else {
                rejected.or(db, &builder, || valid)
            };

            // Only the valid sibling consumes the budget, even when the rejected path had
            // already selected a type or retained a fallback before finding its contradiction.
            for rejected_binding in [
                PathBoundSolution::Solved(str),
                PathBoundSolution::BudgetExceeded {
                    fallback: Some(str),
                },
            ] {
                let choose = |_, bound: &PathBound<'_>| {
                    if bound.bound_typevar == u {
                        PathBoundSolution::Unsatisfiable
                    } else if bound.evidence_lower == Some(str) {
                        rejected_binding
                    } else {
                        PathBoundSolution::Solved(int)
                    }
                };
                assert_eq!(
                    set.solutions_with(db, &env, inferable, budget, choose),
                    Ok(Solutions::Constrained(SolutionPaths::Complete(vec![vec![
                        binding(t, int),
                    ]])))
                );
                assert_eq!(
                    set.try_fold_solutions(
                        db,
                        &env,
                        inferable,
                        budget,
                        choose,
                        Vec::new(),
                        |mut paths, path, budget| {
                            for binding in path {
                                budget.charge_type(db, binding.solution)?;
                            }
                            paths.push(path.to_vec());
                            Ok(paths)
                        },
                    ),
                    Ok(SolutionProjection::Constrained(vec![vec![binding(t, int)]]))
                );
            }
        }
    }
}

#[test]
fn valid_unsolved_path_is_not_unconstrained() {
    let db = setup_db();
    let db = &db;
    let env = db.program_environment();
    let t = create_typevar(db, "T");
    let builder = ConstraintSetBuilder::new();
    let set = exact(db, &builder, t, known_instance(db, KnownClass::Int));
    let inferable = TypeVarSet::from_typevars(db, [t]);
    let budget = SolutionBudget {
        type_terms: 0,
        ..SolutionBudget::default()
    };

    for (selected, collected, projected) in [
        (
            PathBoundSolution::Unsolved,
            Solutions::Constrained(SolutionPaths::Complete(vec![vec![]])),
            Ok(SolutionProjection::Constrained(1)),
        ),
        (
            PathBoundSolution::BudgetExceeded { fallback: None },
            Solutions::Constrained(SolutionPaths::BudgetExceeded(vec![vec![]])),
            Err(ProjectionError::IncompleteSolution),
        ),
        (
            PathBoundSolution::Unsatisfiable,
            Solutions::Unsatisfiable,
            Ok(SolutionProjection::Unsatisfiable),
        ),
    ] {
        assert_eq!(
            set.solutions_with(db, &env, inferable, budget, |_, _| selected),
            Ok(collected)
        );
        assert_eq!(
            set.try_fold_solutions(
                db,
                &env,
                inferable,
                budget,
                |_, _| selected,
                0,
                |count, path, _| {
                    assert!(path.is_empty());
                    Ok(count + 1)
                },
            ),
            projected
        );
    }
}

#[test]
fn type_budget_is_charged_before_constructing_a_union() {
    let db = setup_db();
    let db = &db;
    let env = db.program_environment();
    let t = create_typevar(db, "T");
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let bytes = known_instance(db, KnownClass::Bytes);
    let builder = ConstraintSetBuilder::new();
    let set = binary_choice(db, &builder, t, [int, str])
        .or(db, &builder, || exact(db, &builder, t, bytes));
    let inferable = TypeVarSet::from_typevars(db, [t]);

    for max_type_terms in [0, 1, 2, 3] {
        let budget = SolutionBudget {
            type_terms: max_type_terms,
            ..SolutionBudget::default()
        };
        let mut selected = 0;
        let collected = set.solutions_with(db, &env, inferable, budget, |_, bound| {
            selected += 1;
            PathBounds::default_solve(db, &env, &builder, bound)
        });
        // One additional path is selected to discover that it exceeds the budget; later
        // paths are not solved.
        assert_eq!(selected, (max_type_terms + 1).min(3));

        let mut constructed = 0;
        let result = set.try_fold_solutions(
            db,
            &env,
            inferable,
            budget,
            |_, bound| PathBounds::default_solve(db, &env, &builder, bound),
            Type::Never,
            |accumulated, path, budget| {
                assert_eq!(path.len(), 1);
                let ty = path[0].solution;
                budget.charge_type(db, ty)?;
                constructed += 1;
                Ok(UnionType::from_two_elements(db, &env, accumulated, ty))
            },
        );

        assert_eq!(constructed, max_type_terms);
        if max_type_terms < 3 {
            assert_eq!(collected, Err(ProjectionError::TypeBudgetExceeded));
            assert_eq!(result, Err(ProjectionError::TypeBudgetExceeded));
        } else {
            assert_eq!(
                collected,
                Ok(Solutions::Constrained(SolutionPaths::Complete(
                    [int, str, bytes].map(|ty| vec![binding(t, ty)]).into()
                )))
            );
            assert_eq!(
                result,
                Ok(SolutionProjection::Constrained(UnionType::from_elements(
                    db,
                    &env,
                    [int, str, bytes],
                )))
            );
        }
    }
}

#[test]
fn type_budget_charges_nested_set_theoretic_terms() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
type Alias = int | str
type Recursive = int | Recursive
"#,
    )?;
    let db = &db;
    let env = db.program_environment();
    let file = system_path_to_file(db, "/src/a.py")?;
    let file = ProgramFile::new(db, file, env.program(db));
    let alias = |name| {
        global_symbol(db, file, name)
            .place
            .expect_type()
            .as_type_alias()
            .map(Type::TypeAlias)
            .ok_or_else(|| anyhow::anyhow!("expected alias {name}"))
    };
    let int = known_instance(db, KnownClass::Int);
    let str = known_instance(db, KnownClass::Str);
    let union = UnionType::from_two_elements(db, &env, int, str);
    let intersection =
        IntersectionType::from_elements(db, &env, [int, Type::int_literal(1).negate(db, &env)]);

    // Existing set operations count their members; aliases cannot hide those members. A
    // recursive alias is charged again at the cycle, but its body is expanded only once.
    for (ty, terms) in [
        (union, 3),
        (intersection, 3),
        (alias("Alias")?, 4),
        (alias("Recursive")?, 4),
    ] {
        assert_eq!(
            ProjectionTypeBudget::new(terms - 1).charge_type(db, ty),
            Err(ProjectionError::TypeBudgetExceeded)
        );
        assert_eq!(ProjectionTypeBudget::new(terms).charge_type(db, ty), Ok(()));
    }
    Ok(())
}

#[test]
fn intersection_construction_failure_discards_the_projection() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
"#,
    )?;
    let db = &db;
    let env = db.program_environment();
    let file = system_path_to_file(db, "/src/a.py")?;
    let file = ProgramFile::new(db, file, env.program(db));
    let instance = |name| {
        global_symbol(db, file, name)
            .place
            .expect_type()
            .to_instance_approximation(db, &env)
            .ok_or_else(|| anyhow::anyhow!("expected class {name}"))
    };
    let left = UnionType::from_elements(db, &env, [instance("A")?, instance("B")?]);
    let right =
        UnionType::from_elements(db, &env, [instance("C")?, instance("D")?, instance("E")?]);
    let t = create_typevar(db, "T");
    let builder = ConstraintSetBuilder::new();

    // These classes can overlap, so distributing the intersection requires six DNF terms.
    // Charging the input alone does not prevent that expansion; the fold also needs a bounded
    // intersection constructor.
    for alternatives in [[left, right], [right, left]] {
        let paths = PathBounds::Constrained(
            alternatives
                .map(|ty| Box::new([PathBound::exact(t, ty)]) as Box<[_]>)
                .into(),
        );

        assert_eq!(
            paths.try_fold_with(
                |_, bound| PathBounds::default_solve(db, &env, &builder, bound),
                Type::object(),
                &mut ProjectionTypeBudget::new(7),
                |accumulated, path, budget| {
                    assert_eq!(path.len(), 1);
                    let ty = path[0].solution;
                    budget.charge_type(db, ty)?;
                    IntersectionType::bounded_from_elements(db, &env, [accumulated, ty])
                        .ok_or(ProjectionError::TypeBudgetExceeded)
                },
            ),
            Err(ProjectionError::TypeBudgetExceeded)
        );
    }
    Ok(())
}
