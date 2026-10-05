use ruff_db::files::system_path_to_file;
use ruff_index::Idx;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, Truthiness, UseDefMap, global_scope, place_table, use_def_map};

use super::{ReachabilityCacheKey, ReachabilityEvaluationCache};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};

const TRUE_SOURCE: &str = "\
outer = True
inner = True
if outer:
    if inner:
        value = 1
";

const FALSE_SOURCE: &str = "\
outer = True
inner = False
if outer:
    if inner:
        value = 1
";

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file("/src/primary.py", TRUE_SOURCE)
        .with_file("/src/other_true.py", TRUE_SOURCE)
        .with_file("/src/other_false.py", FALSE_SOURCE)
        .build()
}

fn binding<'db>(
    db: &'db TestDb,
    path: &str,
) -> anyhow::Result<(
    ScopeId<'db>,
    &'db UseDefMap<'db>,
    ScopedReachabilityConstraintId,
)> {
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, path)?,
        db.program_environment().program(db),
    );
    let scope = global_scope(db, file);
    let use_def = use_def_map(db, scope);
    let symbol = place_table(db, scope)
        .symbol_id("value")
        .ok_or_else(|| anyhow::anyhow!("fixture must define value"))?;
    let id = use_def
        .end_of_scope_symbol_bindings(symbol)
        .find_map(|binding| {
            binding
                .binding
                .definition()
                .map(|_| binding.reachability_constraint)
        })
        .ok_or_else(|| anyhow::anyhow!("fixture must bind value"))?;
    Ok((scope, use_def, id))
}

#[test]
fn primary_evaluation_leaves_gap_slots_empty() -> anyhow::Result<()> {
    let db = database()?;
    let (scope, use_def, id) = binding(&db, "/src/primary.py")?;
    let constraints = use_def.reachability_constraints();
    let cache = ReachabilityEvaluationCache::new(scope, constraints);
    let key = cache.key(scope, constraints, id);
    assert_eq!(key, ReachabilityCacheKey::Primary(id.index()));
    assert!(
        id.index() > 0,
        "nested conditions must leave preceding slots"
    );
    assert_eq!(cache.lookup(key), None);
    assert_eq!(cache.storage(key), (0, 0));

    assert_eq!(
        cache.evaluate(&db, constraints, use_def.predicates(), id),
        Truthiness::AlwaysTrue,
    );
    assert_eq!(cache.lookup(key), Some(Truthiness::AlwaysTrue));
    let storage = cache.storage(key);
    assert_eq!(storage.0, id.index() + 1);
    for index in 0..id.index() {
        assert_eq!(cache.lookup(ReachabilityCacheKey::Primary(index)), None);
    }

    assert_eq!(
        cache.evaluate(&db, constraints, use_def.predicates(), id),
        super::evaluate_reachability(&db, &use_def, id),
    );
    assert_eq!(cache.storage(key), storage);
    Ok(())
}

#[test]
fn secondary_evaluations_distinguish_graphs_with_the_same_local_id() -> anyhow::Result<()> {
    let db = database()?;
    let (scope, use_def, primary_id) = binding(&db, "/src/primary.py")?;
    let cache = ReachabilityEvaluationCache::new(scope, use_def.reachability_constraints());
    let mut keys = Vec::new();

    for (path, expected) in [
        ("/src/other_true.py", Truthiness::AlwaysTrue),
        ("/src/other_false.py", Truthiness::AlwaysFalse),
    ] {
        let (other_scope, other_use_def, id) = binding(&db, path)?;
        let constraints = other_use_def.reachability_constraints();
        assert_eq!(id, primary_id);
        let key = cache.key(other_scope, constraints, id);
        assert_eq!(
            key,
            ReachabilityCacheKey::Other {
                constraints: std::ptr::from_ref(constraints).addr(),
                id,
            },
        );
        assert_eq!(cache.lookup(key), None);
        assert_eq!(
            cache.evaluate(&db, constraints, other_use_def.predicates(), id),
            expected,
        );
        assert_eq!(cache.lookup(key), Some(expected));
        keys.push((key, expected));
        assert_eq!(cache.storage(key).0, keys.len());

        let storage = cache.storage(key);
        assert_eq!(
            cache.evaluate(&db, constraints, other_use_def.predicates(), id),
            super::evaluate_reachability(&db, &other_use_def, id),
        );
        assert_eq!(cache.storage(key), storage);
    }

    assert_ne!(keys[0].0, keys[1].0);
    for (key, expected) in keys {
        assert_eq!(cache.lookup(key), Some(expected));
    }
    assert_eq!(cache.storage(ReachabilityCacheKey::Primary(0)), (0, 0));
    Ok(())
}

#[test]
fn primary_storage_requires_both_scope_and_graph_identity() -> anyhow::Result<()> {
    let db = database()?;
    let (scope, use_def, id) = binding(&db, "/src/primary.py")?;
    let (other_scope, other_use_def, other_id) = binding(&db, "/src/other_true.py")?;
    let constraints = use_def.reachability_constraints();
    let other_constraints = other_use_def.reachability_constraints();
    assert_eq!(constraints, other_constraints);
    assert_eq!(id, other_id);
    assert_ne!(scope, other_scope);
    assert!(!std::ptr::eq(constraints, other_constraints));
    let cache = ReachabilityEvaluationCache::new(scope, constraints);
    let primary_key = cache.key(scope, constraints, id);

    // Equal graphs can use the same predicate indices. Changing either the graph or the
    // predicates' scope still excludes the evaluation from the primary dense storage.
    for (evaluation_scope, graph, predicates) in [
        (scope, other_constraints, use_def.predicates()),
        (other_scope, constraints, other_use_def.predicates()),
    ] {
        let key = cache.key(evaluation_scope, graph, id);
        assert_eq!(
            key,
            ReachabilityCacheKey::Other {
                constraints: std::ptr::from_ref(graph).addr(),
                id,
            },
        );
        assert_eq!(cache.lookup(key), None);
        assert_eq!(
            cache.evaluate(&db, graph, predicates, id),
            Truthiness::AlwaysTrue,
        );
        assert_eq!(cache.lookup(key), Some(Truthiness::AlwaysTrue));
        assert_eq!(cache.lookup(primary_key), None);
        assert_eq!(cache.storage(primary_key), (0, 0));
    }
    Ok(())
}

#[test]
fn terminals_do_not_populate_either_cache() -> anyhow::Result<()> {
    let db = database()?;
    let (scope, use_def, id) = binding(&db, "/src/primary.py")?;
    let (other_scope, other_use_def, other_id) = binding(&db, "/src/other_true.py")?;
    let constraints = use_def.reachability_constraints();
    let cache = ReachabilityEvaluationCache::new(scope, constraints);
    let primary_key = cache.key(scope, constraints, id);
    let other_key = cache.key(
        other_scope,
        other_use_def.reachability_constraints(),
        other_id,
    );

    for (id, expected) in [
        (
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            Truthiness::AlwaysTrue,
        ),
        (
            ScopedReachabilityConstraintId::ALWAYS_FALSE,
            Truthiness::AlwaysFalse,
        ),
        (
            ScopedReachabilityConstraintId::AMBIGUOUS,
            Truthiness::Ambiguous,
        ),
    ] {
        for use_def in [&use_def, &other_use_def] {
            assert_eq!(
                cache.evaluate(
                    &db,
                    use_def.reachability_constraints(),
                    use_def.predicates(),
                    id
                ),
                expected,
            );
            assert_eq!(cache.storage(primary_key), (0, 0));
            assert_eq!(cache.storage(other_key), (0, 0));
        }
    }
    Ok(())
}
