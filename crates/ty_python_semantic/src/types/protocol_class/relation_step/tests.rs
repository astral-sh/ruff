//! Representation controls using real protocol members and finite child relations.
//!
//! Child pairs are evaluated synchronously here. These controls do not establish flat recursive
//! dispatch, enclosing cycle-scope cleanup, or a suspended multi-member constraint fold.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{MemberPairDependencies, OrdinaryDependencies, ProtocolMemberAccessPairStep as Step};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::protocol_class::{ProtocolMember, ProtocolMemberAccessMode};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, ClassLiteral, ProtocolInstanceType, Type};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/member_steps.py",
            r#"from typing import Protocol

class IntMember(Protocol):
    value: int
    def method(self) -> int: ...

class ObjectMember(Protocol):
    value: object
    def method(self) -> int: ...

class StrMember(Protocol):
    value: str

class GenericMember[T](Protocol):
    value: T

class ReadOnlyMember[T](Protocol):
    @property
    def value(self) -> T: ...
"#,
        )
        .build()
}

fn protocol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ProtocolInstanceType<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/member_steps.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
    Type::instance(db, &env, class.identity_specialization(db))
        .as_protocol_instance()
        .ok_or_else(|| anyhow::anyhow!("fixture class {name} is not a protocol"))
}

fn member<'db>(
    db: &'db TestDb,
    protocol: ProtocolInstanceType<'db>,
    name: &'static str,
) -> anyhow::Result<ProtocolMember<'db, 'db>> {
    protocol
        .interface(db)
        .member_by_name(db, name)
        .ok_or_else(|| anyhow::anyhow!("missing fixture member {name}"))
}

fn with_checker<'db>(
    db: &'db TestDb,
    inferable: TypeVarSet<'db>,
    check: impl FnOnce(&TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relation_visitor = HasRelationToVisitor::default(&constraints);
    let disjointness_visitor = IsDisjointVisitor::default(&constraints);
    let signature_visitor = SignatureRelationVisitor::default();
    let mapping_visitor = ApplyTypeMappingVisitor::new(&env);
    let checker = TypeRelationChecker::constraint_set_assignability_with_context(
        &env,
        &constraints,
        &relation_visitor,
        &disjointness_visitor,
        &signature_visitor,
        &mapping_visitor,
    )
    .with_inferable_typevars(inferable);
    check(&checker)
}

struct Outcome<'db, 'c> {
    result: ConstraintSet<'db, 'c>,
    requests: Vec<(Type<'db>, Type<'db>)>,
}

fn drive<'c, 'db, D: MemberPairDependencies>(
    db: &'db TestDb,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    source: ProtocolInstanceType<'db>,
    members: (ProtocolMember<'_, 'db>, ProtocolMember<'_, 'db>),
    dependencies: &D,
) -> Result<Outcome<'db, 'c>, D::Error> {
    let mut step = {
        let (source_member, target_member) = members;
        Step::start(
            db,
            checker,
            Type::ProtocolInstance(source),
            &source_member,
            &target_member,
            ProtocolMemberAccessMode::Instance,
            dependencies,
        )?
    };
    let mut requests = Vec::new();
    loop {
        match step {
            Step::Complete(result) => return Ok(Outcome { result, requests }),
            Step::Relate(pending) => {
                assert!(std::ptr::eq(pending.checker, checker));
                requests.push((pending.source, pending.target));
                let result = dependencies.run(db, || {
                    pending
                        .checker
                        .check_type_pair(db, pending.source, pending.target)
                })?;
                step = pending.resume(db, result, dependencies)?;
            }
        }
    }
}

#[test]
fn member_steps_preserve_class_shortcut_and_read_write_direction() -> anyhow::Result<()> {
    let db = database()?;
    let source = protocol(&db, "IntMember")?;
    let target = protocol(&db, "ObjectMember")?;
    with_checker(&db, TypeVarSet::None, |checker| {
        let dependencies = RefusingDependencies::default();
        let shortcut = Step::start(
            &db,
            checker,
            Type::ProtocolInstance(source),
            &member(&db, source, "method")?,
            &member(&db, target, "method")?,
            ProtocolMemberAccessMode::Class,
            &dependencies,
        );
        let Ok(Step::Complete(shortcut)) = shortcut else {
            anyhow::bail!("class-side method presence should complete without a child");
        };
        assert!(shortcut.is_trivially_always_satisfied());
        assert_eq!(dependencies.executed.get(), 1);

        let source_member = member(&db, source, "value")?;
        let target_member = member(&db, target, "value")?;
        let Step::Relate(read) = Step::start(
            &db,
            checker,
            Type::ProtocolInstance(source),
            &source_member,
            &target_member,
            ProtocolMemberAccessMode::Instance,
            &OrdinaryDependencies,
        )?
        else {
            anyhow::bail!("mutable members require a read comparison");
        };
        assert!(std::ptr::eq(read.checker, checker));
        let read_pair = (read.source, read.target);
        assert_ne!(read_pair.0, read_pair.1);
        let read_result = checker.check_type_pair(&db, read.source, read.target);
        assert!(read_result.is_trivially_always_satisfied());
        let Step::Relate(write) = read.resume(&db, read_result, &OrdinaryDependencies)? else {
            anyhow::bail!("mutable members require a write comparison after the read");
        };
        assert!(std::ptr::eq(write.checker, checker));
        assert_eq!((write.source, write.target), (read_pair.1, read_pair.0));
        let write_result = checker.check_type_pair(&db, write.source, write.target);
        assert!(write_result.is_trivially_never_satisfied());
        let Step::Complete(result) = write.resume(&db, write_result, &OrdinaryDependencies)? else {
            anyhow::bail!("read and write complete this member");
        };
        assert!(result.ownership_probe_same_set(write_result));
        Ok(())
    })
}

#[test]
fn member_steps_retain_conditional_constraints() -> anyhow::Result<()> {
    let db = database()?;
    let source = protocol(&db, "IntMember")?;
    for target_name in ["GenericMember", "ReadOnlyMember"] {
        let target = protocol(&db, target_name)?;
        let target_member = member(&db, target, "value")?;
        let env = db.program_environment();
        let required = target_member
            .access(&db, &env, ProtocolMemberAccessMode::Instance)
            .read
            .and_then(|read| read.resolve(&db, &env))
            .ok_or_else(|| anyhow::anyhow!("generic fixture requires a readable type"))?;
        let Type::TypeVar(variable) = required.ty() else {
            anyhow::bail!("identity specialization must retain the member type variable");
        };
        with_checker(&db, TypeVarSet::from_typevars(&db, [variable]), |checker| {
            let Step::Relate(read) = Step::start(
                &db,
                checker,
                Type::ProtocolInstance(source),
                &member(&db, source, "value")?,
                &target_member,
                ProtocolMemberAccessMode::Instance,
                &OrdinaryDependencies,
            )?
            else {
                anyhow::bail!("generic member requires a read comparison");
            };
            assert!(std::ptr::eq(read.checker, checker));
            assert_eq!(read.target, Type::TypeVar(variable));
            let read_pair = (read.source, read.target);
            let read_result = checker.check_type_pair(&db, read.source, read.target);
            assert!(!read_result.is_trivially_always_satisfied());
            assert!(!read_result.is_trivially_never_satisfied());
            match read.resume(&db, read_result, &OrdinaryDependencies)? {
                Step::Complete(result) => {
                    assert_eq!(target_name, "ReadOnlyMember");
                    assert!(result.ownership_probe_same_set(read_result));
                }
                Step::Relate(write) => {
                    assert_eq!(target_name, "GenericMember");
                    assert!(std::ptr::eq(write.checker, checker));
                    assert_eq!((write.source, write.target), (read_pair.1, read_pair.0));
                    let write_result = checker.check_type_pair(&db, write.source, write.target);
                    let expected = read_result.and(&db, checker.constraints, || write_result);
                    let Step::Complete(result) =
                        write.resume(&db, write_result, &OrdinaryDependencies)?
                    else {
                        anyhow::bail!("generic write must complete this member");
                    };
                    assert!(result.ownership_probe_same_set(expected));
                    assert!(!result.is_trivially_always_satisfied());
                    assert!(!result.is_trivially_never_satisfied());
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn negative_member_read_skips_write_dependencies() -> anyhow::Result<()> {
    let db = database()?;
    let source = protocol(&db, "IntMember")?;
    let target = protocol(&db, "StrMember")?;
    with_checker(&db, TypeVarSet::None, |checker| {
        let checker = checker.with_context_collection_disabled();
        let dependencies = RefusingDependencies::default();
        let Step::Relate(read) = Step::start(
            &db,
            &checker,
            Type::ProtocolInstance(source),
            &member(&db, source, "value")?,
            &member(&db, target, "value")?,
            ProtocolMemberAccessMode::Instance,
            &OrdinaryDependencies,
        )?
        else {
            anyhow::bail!("different readable member types require comparison");
        };
        let read_result = checker.check_type_pair(&db, read.source, read.target);
        assert!(read_result.is_trivially_never_satisfied());
        let Ok(Step::Complete(result)) = read.resume(&db, read_result, &dependencies) else {
            anyhow::bail!("negative read must complete before any write dependency");
        };
        assert!(result.ownership_probe_same_set(read_result));
        assert_eq!(dependencies.calls.get(), 0);
        Ok(())
    })
}

#[derive(Clone, Copy)]
enum RefusalPhase {
    Before,
    After,
}

#[derive(Debug, Eq, PartialEq)]
struct Refused;

#[derive(Default)]
struct RefusingDependencies {
    refuse: Option<(usize, RefusalPhase)>,
    calls: Cell<usize>,
    executed: Cell<usize>,
    stopped: Cell<bool>,
}

impl MemberPairDependencies for RefusingDependencies {
    type Error = Refused;

    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Refused> {
        let index = self.calls.get() + 1;
        self.calls.set(index);
        if self.stopped.get()
            || matches!(self.refuse, Some((at, RefusalPhase::Before)) if at == index)
        {
            self.stopped.set(true);
            return Err(Refused);
        }
        let value = operation();
        self.executed.set(self.executed.get() + 1);
        if matches!(self.refuse, Some((at, RefusalPhase::After)) if at == index) {
            self.stopped.set(true);
            return Err(Refused);
        }
        Ok(value)
    }
}

#[test]
fn member_dependency_refusal_prevents_consumption_and_allows_retries() -> anyhow::Result<()> {
    let db = database()?;
    let source = protocol(&db, "IntMember")?;
    with_checker(&db, TypeVarSet::None, |checker| {
        let baseline_dependencies = RefusingDependencies::default();
        let Ok(baseline) = drive(
            &db,
            checker,
            source,
            (member(&db, source, "value")?, member(&db, source, "value")?),
            &baseline_dependencies,
        ) else {
            anyhow::bail!("unrestricted member comparison must complete");
        };
        assert_eq!(baseline.requests.len(), 2);
        assert!(baseline.result.is_trivially_always_satisfied());
        let dependency_count = baseline_dependencies.calls.get();
        assert!(dependency_count > baseline.requests.len());
        for index in 1..=dependency_count {
            for phase in [RefusalPhase::Before, RefusalPhase::After] {
                let dependencies = RefusingDependencies {
                    refuse: Some((index, phase)),
                    ..RefusingDependencies::default()
                };
                assert!(matches!(
                    drive(
                        &db,
                        checker,
                        source,
                        (member(&db, source, "value")?, member(&db, source, "value")?),
                        &dependencies,
                    ),
                    Err(Refused)
                ));
                assert!(dependencies.stopped.get());
                assert_eq!(dependencies.calls.get(), index);
                assert_eq!(
                    dependencies.executed.get(),
                    match phase {
                        RefusalPhase::Before => index - 1,
                        RefusalPhase::After => index,
                    }
                );
                for _ in 0..2 {
                    let retry = drive(
                        &db,
                        checker,
                        source,
                        (member(&db, source, "value")?, member(&db, source, "value")?),
                        &OrdinaryDependencies,
                    )?;
                    assert_eq!(retry.requests, baseline.requests);
                    assert!(retry.result.ownership_probe_same_set(baseline.result));
                }
            }
        }
        Ok(())
    })
}
