use std::future::{Future, pending};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::task::{Context, Waker};
use std::thread;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use rustc_hash::{FxHashMap, FxHasher};
use ty_python_core::ProgramFile;

use super::{
    ContextCommitConflict, ErrorContext, ErrorContextNode, ErrorContextTree, ErrorRelation,
    FrozenErrorContextTree, ParameterDescription,
};
use crate::db::tests::{TestDb, setup_db};
use crate::place::global_symbol;
use crate::types::known_instance::{MethodWrapper, MethodWrapperKind};
use crate::types::relation::TypeRelation;
use crate::types::{KnownInstanceType, Type};
use crate::{Db, FxOrderSet, ProgramEnvironment};

fn fixture() -> anyhow::Result<TestDb> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/evidence.py",
        r#"
        def source(value: int) -> str: ...
        def target(value: int) -> int: ...
    "#,
    )?;
    Ok(db)
}

fn failed_callable<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
) -> anyhow::Result<(ErrorContextTree<'db>, Type<'db>, Type<'db>)> {
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/evidence.py")?,
        env.program(db),
    );
    let source_function = global_symbol(db, file, "source")
        .place
        .expect_type()
        .expect_function_literal();
    let target_function = global_symbol(db, file, "target")
        .place
        .expect_type()
        .expect_function_literal();
    let source = Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
        db,
        Type::FunctionLiteral(source_function),
        MethodWrapperKind::Staticmethod,
    )));
    let target = Type::Callable(target_function.into_callable_type(db));
    assert!(!source.is_assignable_to(db, env, target));
    let context = source.assignability_error_context(db, env, target);
    let root = Rc::clone(&context.root.borrow().node);
    assert!(
        matches!(root.context, ErrorContext::InferredCallableType { source: actual, .. } if actual == source)
    );
    let failure = root
        .children
        .iter()
        .find_map(|child| match child.context {
            ErrorContext::IncompatibleReturnTypes {
                source_definition,
                target_definition,
                ..
            } => Some((source_definition, target_definition)),
            _ => None,
        })
        .expect("real callable comparison records its return mismatch");
    assert_eq!(
        failure,
        (
            Some(source_function.definition(db)),
            Some(target_function.definition(db))
        )
    );
    drop(root);
    Ok((context, source, target))
}

fn rendered<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    tree: &ErrorContextTree<'db>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut help = FxOrderSet::default();
    tree.root
        .borrow()
        .node
        .render_tree(db, env, &mut lines, &mut help, "", "");
    assert!(!lines.is_empty());
    lines
}

fn parent_context<'db>(source: Type<'db>, target: Type<'db>, index: usize) -> ErrorContext<'db> {
    ErrorContext::IncompatibleParameterTypes {
        source,
        target,
        parameter: ParameterDescription::Index(index),
    }
}

#[test]
fn frozen_relation_evidence_subscribers_are_independent() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let (completed, source, target) = failed_callable(&db, &env)?;
    let expected = completed.root.borrow().node.clone();
    let expected_lines = rendered(&db, &env, &completed);
    let child_buffer = completed.root.borrow().node.children.as_ptr();
    assert_eq!(Rc::strong_count(&completed.root), 1);
    let frozen = completed.freeze();
    assert_eq!(frozen.root.children.as_ptr(), child_buffer);
    let retained = frozen.clone();
    assert!(Rc::ptr_eq(&retained.root, &frozen.root));

    let first = frozen.instantiate();
    let second = frozen.instantiate();
    assert!(!Rc::ptr_eq(&first.root, &second.root));
    assert!(Rc::ptr_eq(&first.root.borrow().node, &frozen.root));
    assert!(Rc::ptr_eq(&second.snapshot().root, &frozen.root));
    let moved = first.take();
    assert!(first.is_empty());
    assert_eq!(second.root.borrow().node, expected);
    let first_parent = ErrorContextTree::new(TypeRelation::Assignability);
    first_parent.set(parent_context(source, target, 0), [moved]);
    assert_eq!(second.root.borrow().node, expected);
    let replacement = frozen.instantiate();
    first_parent.replace(&replacement);
    assert!(replacement.is_empty());
    assert_eq!(second.root.borrow().node, expected);
    assert_eq!(rendered(&db, &env, &second), expected_lines);
    let second_parent = ErrorContextTree::new(TypeRelation::Assignability);
    second_parent.set(parent_context(source, target, 1), [second]);
    assert_eq!(
        second_parent.root.borrow().node.children.as_ref(),
        std::slice::from_ref(&expected)
    );
    drop(first_parent);
    drop(second_parent);
    assert_eq!(retained.instantiate().root.borrow().node, expected);
    assert_eq!(rendered(&db, &env, &retained.instantiate()), expected_lines);
    Ok(())
}

fn leaf(index: usize) -> ErrorContextTree<'static> {
    ErrorContextTree::from_context(
        ErrorContext::MissingParameter {
            parameter: ParameterDescription::Index(index),
        },
        TypeRelation::Assignability,
    )
}

fn seed(reverse_allocation: bool, reverse_children: bool) -> FrozenErrorContextTree<'static> {
    let (first, second) = if reverse_allocation {
        let second = leaf(1);
        (leaf(0), second)
    } else {
        let first = leaf(0);
        (first, leaf(1))
    };
    let tree = ErrorContextTree::new(ErrorRelation::Disjointness);
    tree.set(
        ErrorContext::NotAssignableToNOtherUnionElements { n: 2 },
        if reverse_children {
            [second, first]
        } else {
            [first, second]
        },
    );
    tree.freeze()
}

fn hash(value: &impl Hash) -> u64 {
    let mut hasher = FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn frozen_relation_persistent_seed_identity_ignores_allocation_order() {
    let first = seed(false, false);
    let second = seed(true, false);
    let reordered = seed(false, true);
    assert!(!Rc::ptr_eq(&first.root, &second.root));
    assert_eq!(first, second);
    assert_eq!(hash(&first), hash(&second));
    assert_ne!(first, reordered);
    for reverse in [false, true] {
        let mut seeds = FxHashMap::default();
        let inputs = if reverse {
            [&reordered, &second, &first]
        } else {
            [&first, &second, &reordered]
        };
        for input in inputs {
            *seeds.entry(input.clone()).or_insert(0) += 1;
        }
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[&first], 2);
        assert_eq!(seeds[&reordered], 1);
    }

    let disabled = first.instantiate();
    disabled.set_enabled(false);
    assert_ne!(first, disabled.snapshot());
    let mut other_relation = first.instantiate();
    other_relation.relation = TypeRelation::Subtyping.into();
    assert_ne!(first, other_relation.snapshot());

    let mut colliding = ErrorContextNode::new(
        ErrorContext::Empty,
        ErrorRelation::Disjointness,
        Box::default(),
    );
    colliding.fingerprint = first.root.fingerprint;
    assert_ne!(first.root.as_ref(), &colliding);
}

#[test]
fn frozen_relation_persistent_alias_drains_preserve_operation_order() {
    let child = leaf(0);
    let before = child.snapshot();
    let parent = ErrorContextTree::new(TypeRelation::Subtyping);
    parent.set(ErrorContext::Empty, [child.clone(), child.clone()]);
    assert!(child.is_empty());
    assert_eq!(parent.root.borrow().node.children.len(), 1);
    assert!(Rc::ptr_eq(
        &parent.root.borrow().node.children[0],
        &before.root
    ));

    let prior = parent.snapshot();
    parent.set(
        ErrorContext::MissingVariadicKeywordParameter,
        [parent.clone()],
    );
    assert!(Rc::ptr_eq(
        &parent.root.borrow().node.children[0],
        &prior.root
    ));
    let prior = parent.snapshot();
    parent.replace(&parent);
    assert_eq!(parent.snapshot(), prior);

    let enabled_alias = parent.clone();
    parent.set_enabled(false);
    assert!(enabled_alias.is_enabled());
    let other = leaf(1);
    parent.set(ErrorContext::Empty, [other.clone()]);
    parent.replace(&other);
    assert!(!other.is_empty());
    enabled_alias.push(ErrorContext::MissingVariadicPositionalParameter);
    assert_eq!(parent, enabled_alias);
    assert_ne!(parent.snapshot(), enabled_alias.snapshot());
    let drained = parent.take();
    assert!(!drained.is_enabled());
    assert!(enabled_alias.is_empty());
    assert_eq!(
        enabled_alias.root.borrow().node.relation,
        TypeRelation::Assignability.into()
    );

    let disabled_source = leaf(2);
    disabled_source.set_enabled(false);
    enabled_alias.replace(&disabled_source);
    assert!(disabled_source.is_empty());
    assert!(!enabled_alias.is_empty());
}

#[test]
fn frozen_relation_persistent_commit_requires_unchanged_original_root() {
    let caller = leaf(0);
    let alias = caller.clone();
    let (seed, lease) = caller.snapshot_with_lease();
    let child = seed.instantiate();
    child.push(ErrorContext::MissingVariadicKeywordParameter);
    let completed = child.freeze();
    assert!(Rc::ptr_eq(&completed.root.children[0], &seed.root));
    assert_eq!(
        seed.instantiate().commit(&completed, &lease),
        Err(ContextCommitConflict)
    );
    assert_eq!(caller.commit(&completed, &lease), Ok(()));
    assert_eq!(alias.snapshot(), completed);
    assert_eq!(
        caller.commit(&completed, &lease),
        Err(ContextCommitConflict)
    );

    let (seed, lease) = caller.snapshot_with_lease();
    let taken = alias.take();
    alias.replace(&taken);
    assert_eq!(caller.snapshot(), seed);
    assert_eq!(caller.commit(&seed, &lease), Err(ContextCommitConflict));
    let (seed, lease) = caller.snapshot_with_lease();
    caller.set_enabled(false);
    caller.set_enabled(true);
    assert_eq!(caller.commit(&seed, &lease), Err(ContextCommitConflict));
    let (seed, lease) = caller.snapshot_with_lease();
    let disabled = seed.instantiate();
    disabled.set_enabled(false);
    assert_eq!(
        caller.commit(&disabled.freeze(), &lease),
        Err(ContextCommitConflict)
    );
    assert_eq!(caller.snapshot(), seed);

    caller.root.borrow_mut().version = u64::MAX;
    let (seed, lease) = caller.snapshot_with_lease();
    assert_eq!(caller.commit(&seed, &lease), Err(ContextCommitConflict));
}

fn chain(depth: usize) -> FrozenErrorContextTree<'static> {
    let tree = leaf(0);
    for _ in 0..depth {
        tree.push(ErrorContext::MissingVariadicPositionalParameter);
    }
    tree.freeze()
}

fn diamond(depth: usize) -> FrozenErrorContextTree<'static> {
    let mut result = leaf(0).freeze();
    for _ in 0..depth {
        let next = ErrorContextTree::new(TypeRelation::Assignability);
        next.set(
            ErrorContext::MissingVariadicKeywordParameter,
            [result.instantiate(), result.instantiate()],
        );
        result = next.freeze();
    }
    result
}

#[test]
fn frozen_relation_persistent_deep_equality_drop_and_cancellation() -> anyhow::Result<()> {
    thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let first = chain(50_000);
            let second = chain(50_000);
            assert_eq!(first, second);
            assert_eq!(hash(&first), hash(&second));
            assert!(!format!("{first:?}").is_empty());
            let first_diamond = diamond(40);
            let second_diamond = diamond(40);
            assert_eq!(first_diamond, second_diamond);
            assert_eq!(hash(&first_diamond), hash(&second_diamond));
            let unshared = ErrorContextTree::new(TypeRelation::Assignability);
            unshared.set(
                ErrorContext::MissingVariadicKeywordParameter,
                [leaf(0), leaf(0)],
            );
            assert_eq!(diamond(1), unshared.freeze());
            let caller = second.instantiate();
            let (seed, lease) = caller.snapshot_with_lease();
            let mut task = Box::pin(async move {
                let private = seed.instantiate();
                private.push(ErrorContext::MissingVariadicKeywordParameter);
                pending::<()>().await;
                drop((private, lease));
            });
            let mut cx = Context::from_waker(Waker::noop());
            assert!(task.as_mut().poll(&mut cx).is_pending());
            drop(task);
            assert_eq!(caller.snapshot(), second);
            drop((caller, first, second, first_diamond, second_diamond));
        })?
        .join()
        .expect("deep diagnostic operations finish on a small stack");
    Ok(())
}

#[test]
fn frozen_relation_persistent_iterative_render_preserves_order() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let tree = seed(false, false).instantiate();
    let lines = rendered(&db, &env, &tree);
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("├── "));
    assert!(lines[1].contains("first parameter"));
    assert!(lines[2].starts_with("└── "));
    assert!(lines[2].contains("second parameter"));
    let deep = chain(2_048).instantiate();
    let lines = rendered(&db, &env, &deep);
    assert_eq!(lines.len(), 2_049);
    assert!(lines[2_048].starts_with(&"    ".repeat(2_047)));
    assert!(lines[2_048].contains("first parameter"));
    Ok(())
}

#[test]
fn frozen_relation_evidence_attachment_order_and_cancellation() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let (completed, source, target) = failed_callable(&db, &env)?;
    let frozen = completed.freeze();
    let expected = frozen.instantiate();
    let mut baseline = None;
    for reverse in [false, true] {
        let parents = [
            ErrorContextTree::new(TypeRelation::Assignability),
            ErrorContextTree::new(TypeRelation::Subtyping),
        ];
        for index in if reverse { [1, 0] } else { [0, 1] } {
            parents[index].set(
                parent_context(source, target, index),
                [frozen.instantiate()],
            );
        }
        let explanations = parents.each_ref().map(|parent| {
            (
                parent.root.borrow().node.clone(),
                rendered(&db, &env, parent),
            )
        });
        if let Some(baseline) = &baseline {
            assert_eq!(&explanations, baseline);
        } else {
            baseline = Some(explanations);
        }

        let subscribers = [frozen.instantiate(), frozen.instantiate()];
        let [first, second] = subscribers;
        let (cancelled, survivor) = if reverse {
            (second, first)
        } else {
            (first, second)
        };
        let mut task = Box::pin(async move {
            let subscriber = cancelled;
            pending::<()>().await;
            drop(subscriber);
        });
        let mut cx = Context::from_waker(Waker::noop());
        assert!(task.as_mut().poll(&mut cx).is_pending());
        drop(task);
        assert_eq!(survivor, expected);
        drop(survivor);
        assert_eq!(frozen.instantiate(), expected);
    }
    Ok(())
}

#[test]
fn frozen_relation_evidence_preserves_aliases_relations_and_enabled_state() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let (completed, source, target) = failed_callable(&db, &env)?;
    let alias = completed.clone();
    let expected = alias.root.borrow().node.clone();
    assert_eq!(Rc::strong_count(&completed.root), 2);
    let frozen = completed.freeze();
    assert_eq!(alias.root.borrow().node, expected);
    let taken = alias.take();
    assert!(alias.is_empty());
    assert_eq!(frozen.instantiate().root.borrow().node, expected);

    let parent = ErrorContextTree::new(ErrorRelation::Disjointness);
    parent.set(parent_context(source, target, 0), [taken]);
    parent.set_enabled(false);
    let frozen_parent = parent.freeze();
    let subscriber = frozen_parent.instantiate();
    assert!(!subscriber.is_enabled());
    assert_eq!(subscriber.relation, ErrorRelation::Disjointness);
    assert_eq!(
        subscriber.root.borrow().node.relation,
        ErrorRelation::Disjointness
    );
    assert_eq!(subscriber.root.borrow().node.children[0], expected);
    assert_eq!(
        subscriber.root.borrow().node.children[0].relation,
        TypeRelation::Assignability.into()
    );
    let before = subscriber.root.borrow().node.clone();
    subscriber.push(ErrorContext::Empty);
    subscriber.set(ErrorContext::Empty, []);
    subscriber.replace(&frozen.instantiate());
    assert_eq!(subscriber.root.borrow().node, before);
    let taken = subscriber.take();
    assert!(subscriber.is_empty());
    assert!(!taken.is_enabled());
    assert_eq!(taken.relation, ErrorRelation::Disjointness);
    assert_eq!(taken.root.borrow().node, before);
    assert_eq!(frozen_parent.instantiate().root.borrow().node, before);
    Ok(())
}
