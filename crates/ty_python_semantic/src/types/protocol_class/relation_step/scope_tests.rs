//! Ownership controls for two nested real member operations and their surrounding visits.

use std::ops::ControlFlow;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{OrdinaryDependencies, ProtocolMemberAccessPairStep as Step};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::cyclic::CycleDetectorVisit;
use crate::types::protocol_class::ProtocolMemberAccessMode;
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::relation_error::ErrorContextTree;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::{ApplyTypeMappingVisitor, ClassLiteral, Type};

fn protocol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/nested_member.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
    let ty = Type::instance(db, &env, class.identity_specialization(db));
    anyhow::ensure!(
        ty.as_protocol_instance().is_some(),
        "fixture must be a protocol"
    );
    Ok(ty)
}

fn nested<'c, 'db>(
    db: &'db TestDb,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    visitor: &HasRelationToVisitor<'db, 'c>,
    source: Type<'db>,
    target: Type<'db>,
    cancel: bool,
) -> anyhow::Result<Option<ConstraintSet<'db, 'c>>> {
    let key = |source, target| (source, target, checker.relation, checker.typevar_evaluation);
    let reuse = |result: &ConstraintSet<'db, 'c>| {
        !checker.is_context_collection_enabled() || !result.is_never_satisfied(db, checker.env)
    };
    let root_visit = match visitor.try_begin_visit(db, key(source, target), reuse) {
        CycleDetectorVisit::Ready(result) => return Ok(Some(result)),
        CycleDetectorVisit::Pending(scope) => scope,
        CycleDetectorVisit::Cycle(_) => {
            anyhow::bail!("fixture root unexpectedly shares an identity")
        }
    };

    let start = |source: Type<'db>, target: Type<'db>, name| {
        let source_member = source
            .as_protocol_instance()
            .and_then(|protocol| protocol.interface(db).member_by_name(db, name))
            .ok_or_else(|| anyhow::anyhow!("source fixture requires {name}"))?;
        let target_member = target
            .as_protocol_instance()
            .and_then(|protocol| protocol.interface(db).member_by_name(db, name))
            .ok_or_else(|| anyhow::anyhow!("target fixture requires {name}"))?;
        let Step::Relate(pending) = Step::start(
            db,
            checker,
            source,
            &source_member,
            &target_member,
            ProtocolMemberAccessMode::Instance,
            &OrdinaryDependencies,
        )?
        else {
            anyhow::bail!("fixture member requires a read comparison");
        };
        anyhow::Ok(pending)
    };
    let root = start(source, target, "child")?;
    let child_key = key(root.source, root.target);
    let CycleDetectorVisit::Pending(child_visit) = visitor.try_begin_visit(db, child_key, reuse)
    else {
        anyhow::bail!("nested fixture requires a fresh or recomputed visit");
    };
    let child = start(root.source, root.target, "value")?;
    assert!(std::ptr::eq(root.checker, checker));
    assert!(std::ptr::eq(child.checker, checker));
    assert_eq!(visitor.ownership_probe_counts().0, 2);

    // An exact active revisit keeps the existing relation visitor's cycle assumption.
    let CycleDetectorVisit::Ready(cycle) = visitor.try_begin_visit(db, child_key, reuse) else {
        anyhow::bail!("an exact active visit must not allocate another scope");
    };
    assert!(cycle.is_trivially_always_satisfied());
    assert_eq!(visitor.ownership_probe_counts().0, 2);

    if cancel {
        drop(child_visit);
        assert_eq!(visitor.ownership_probe_counts().0, 1);
        drop(root_visit);
        assert_eq!(visitor.ownership_probe_counts().0, 0);
        return Ok(None);
    }

    // Only the primitive leaf uses ordinary dispatch; both protocol parents remain suspended.
    let leaf = checker.check_type_pair(db, child.source, child.target);
    let Step::Complete(child_result) = child.resume(db, leaf, &OrdinaryDependencies)? else {
        anyhow::bail!("read-only child must complete after its read");
    };
    assert!(child_result.ownership_probe_same_set(leaf));
    let child_result = child_visit.finish(child_result);
    assert_eq!(visitor.ownership_probe_counts().0, 1);
    let Step::Complete(result) = root.resume(db, child_result, &OrdinaryDependencies)? else {
        anyhow::bail!("read-only root must complete after its child");
    };
    assert!(result.ownership_probe_same_set(child_result));
    let result = root_visit.finish(result);
    assert_eq!(visitor.ownership_probe_counts().0, 0);
    Ok(Some(result))
}

#[test]
fn nested_member_visits_cancel_retry_and_recompute() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/nested_member.py",
            r"
from typing import Protocol

class SourceLeaf(Protocol):
    @property
    def value(self) -> int: ...

class TargetLeaf(Protocol):
    @property
    def value(self) -> str: ...

class SourceRoot(Protocol):
    @property
    def child(self) -> SourceLeaf: ...

class TargetRoot(Protocol):
    @property
    def child(self) -> TargetLeaf: ...
",
        )
        .build()?;
    let env = db.program_environment();
    let source = protocol(&db, "SourceRoot")?;
    let target = protocol(&db, "TargetRoot")?;
    assert!(!source.is_assignable_to(&db, &env, target));

    let constraints = ConstraintSetBuilder::new();
    let relation_visitor = HasRelationToVisitor::default(&constraints);
    let disjointness_visitor = IsDisjointVisitor::default(&constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mapping_visitor = ApplyTypeMappingVisitor::new(&env);
    let checker = TypeRelationChecker::assignability_with_context(
        &env,
        &constraints,
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    );
    let quiet = checker.with_context_collection_disabled();
    let mut fold = ConstraintFold::new(&constraints, ConstraintFoldKind::All);
    assert!(matches!(
        fold.push(checker.always()),
        ControlFlow::Continue(())
    ));
    assert!(nested(&db, &quiet, &relation_visitor, source, target, true)?.is_none());
    assert_eq!(relation_visitor.ownership_probe_counts(), (0, 0));

    // Two unchanged-revision retries reuse the original visitor and constraint storage.
    let mut cached = None;
    for _ in 0..2 {
        let result = nested(&db, &quiet, &relation_visitor, source, target, false)?
            .ok_or_else(|| anyhow::anyhow!("retry must complete"))?;
        assert!(result.is_trivially_never_satisfied());
        if let Some(previous) = cached {
            assert!(result.ownership_probe_same_set(previous));
        }
        cached = Some(result);
    }
    let cache_count = relation_visitor.ownership_probe_counts().1;
    assert!(cache_count >= 2);
    assert!(
        checker
            .report_context()
            .is_some_and(ErrorContextTree::is_empty)
    );
    let result = nested(&db, &checker, &relation_visitor, source, target, false)?
        .ok_or_else(|| anyhow::anyhow!("context recomputation must complete"))?;
    assert_eq!(relation_visitor.ownership_probe_counts(), (0, cache_count));
    assert!(
        checker
            .report_context()
            .is_some_and(|context| !context.is_empty())
    );
    assert!(matches!(fold.push(result), ControlFlow::Break(_)));
    Ok(())
}
