//! Finite controls for pending member reads; child requests use ordinary semantic evaluation.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::ProtocolMemberReadStep as Step;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::protocol_class::nominal_member_step::NominalMember;
use crate::types::protocol_class::relation_step::{MemberPairDependencies, OrdinaryDependencies};
use crate::types::protocol_class::{
    ProtocolMember, ProtocolMemberAccessMode, protocol_member_read_type,
};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, ClassLiteral, MemberLookupPolicy, Type, UpcastPolicy};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/nominal_reads.py",
            r#"from typing import Protocol

class IntRead(Protocol):
    @property
    def value(self) -> int: ...

class ObjectRead(Protocol):
    @property
    def value(self) -> object: ...

class GenericRead[T](Protocol):
    @property
    def value(self) -> T: ...

class Method(Protocol):
    def method(self, value: int) -> str: ...

class ClassMethod(Protocol):
    @classmethod
    def method(cls, value: int) -> str: ...

class Data:
    value: int

class Missing: ...

class Good:
    def method(self, value: int) -> str: ...

class Wrong:
    def method(self, value: str) -> str: ...

class Zero:
    @staticmethod
    def method() -> str: ...

class Class:
    @classmethod
    def method(cls, value: int) -> str: ...

flag: bool
class Alternatives:
    method = Good.method if flag else Wrong.method
"#,
        )
        .build()
}

fn instance<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/nominal_reads.py")?,
        env.program(db),
    );
    let class = global_symbol(db, file, name)
        .place
        .expect_type()
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
    Ok(Type::instance(db, &env, class.identity_specialization(db)))
}

fn member<'db>(
    db: &'db TestDb,
    protocol: &str,
    name: &'static str,
) -> anyhow::Result<ProtocolMember<'db, 'db>> {
    instance(db, protocol)?
        .as_protocol_instance()
        .and_then(|protocol| protocol.interface(db).member_by_name(db, name))
        .ok_or_else(|| anyhow::anyhow!("missing fixture member {protocol}.{name}"))
}

fn with_checker<'db>(
    db: &'db TestDb,
    inferable: TypeVarSet<'db>,
    check: impl FnOnce(&TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let relation = HasRelationToVisitor::default(&builder);
    let disjointness = IsDisjointVisitor::default(&builder);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    let checker = TypeRelationChecker::constraint_set_assignability_with_context(
        &env,
        &builder,
        &relation,
        &disjointness,
        &signatures,
        &mapping,
    )
    .with_inferable_typevars(inferable);
    check(&checker)
}

fn drive<'db, 'c, D: MemberPairDependencies>(
    db: &'db TestDb,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    ty: Type<'db>,
    member: ProtocolMember<'_, 'db>,
    access: ProtocolMemberAccessMode,
    dependencies: &D,
) -> Result<(ConstraintSet<'db, 'c>, Vec<&'static str>), D::Error> {
    let required = dependencies.run(db, || member.access(db, checker.env, access).read)?;
    let Some(required) = required else {
        return Ok((checker.always(), vec![]));
    };
    let receiver = match access {
        ProtocolMemberAccessMode::Instance => ty,
        ProtocolMemberAccessMode::Class => {
            dependencies.run(db, || ty.to_meta_type(db, checker.env))?
        }
    };
    let mut step = Step::start(
        db,
        NominalMember {
            checker,
            ty,
            member,
        },
        receiver,
        required,
        access,
        dependencies,
    )?;
    let mut requests = Vec::new();
    loop {
        step = match step {
            Step::Complete(result) => return Ok((result, requests)),
            Step::Presence(pending) => {
                requests.push("presence");
                let present = dependencies.run(db, || {
                    pending
                        .receiver()
                        .member_lookup_with_policy(
                            db,
                            checker.env,
                            pending.name(),
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .place
                        .is_definitely_bound()
                })?;
                pending.resume(present)
            }
            Step::Lookup(pending) => {
                requests.push("lookup");
                let result = dependencies.run(db, || {
                    protocol_member_read_type(
                        db,
                        checker.env,
                        pending.candidate(),
                        pending.receiver(),
                        pending.member(),
                        pending.access(),
                    )
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::Convert(pending) => {
                requests.push("convert");
                assert!(std::ptr::eq(pending.checker, checker));
                let result = dependencies.run(db, || {
                    pending.source.try_upcast_to_callable_with_policy(
                        db,
                        checker.env,
                        UpcastPolicy::from(checker.relation),
                    )
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::Relate(pending) => {
                requests.push("relate");
                assert!(std::ptr::eq(pending.checker, checker));
                let result = dependencies.run(db, || {
                    pending
                        .checker
                        .check_type_pair(db, pending.source, pending.target)
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::Callables(pending) => {
                requests.push("callables");
                assert!(std::ptr::eq(pending.checker, checker));
                let result = dependencies.run(db, || {
                    pending
                        .checker
                        .check_callables_vs_callable(db, &pending.source, pending.target)
                })?;
                Step::Complete(result)
            }
            Step::Callable(pending) => {
                requests.push("callable");
                assert!(std::ptr::eq(pending.checker(), checker));
                let result = dependencies.run(db, || {
                    pending
                        .checker()
                        .check_callable_pair(db, pending.source, pending.target)
                })?;
                pending.resume(db, result, dependencies)?
            }
        };
    }
}

#[test]
fn nominal_reads_preserve_data_and_method_semantics() -> anyhow::Result<()> {
    let db = database()?;
    let instance_access = ProtocolMemberAccessMode::Instance;
    let class_access = ProtocolMemberAccessMode::Class;
    for (candidate, protocol, name, access, valid, steps) in [
        (
            "Data",
            "ObjectRead",
            "value",
            instance_access,
            true,
            vec!["presence"],
        ),
        (
            "Data",
            "IntRead",
            "value",
            instance_access,
            true,
            vec!["lookup", "relate"],
        ),
        (
            "Missing",
            "IntRead",
            "value",
            instance_access,
            false,
            vec!["lookup"],
        ),
        (
            "Good",
            "Method",
            "method",
            instance_access,
            true,
            vec!["lookup", "convert", "callables"],
        ),
        (
            "Wrong",
            "Method",
            "method",
            instance_access,
            false,
            vec!["lookup", "convert", "callables"],
        ),
        (
            "Good",
            "Method",
            "method",
            class_access,
            true,
            vec!["lookup", "convert", "callable"],
        ),
        (
            "Zero",
            "Method",
            "method",
            class_access,
            false,
            vec!["lookup", "convert"],
        ),
        (
            "Class",
            "ClassMethod",
            "method",
            class_access,
            true,
            vec!["lookup", "relate"],
        ),
    ] {
        with_checker(&db, TypeVarSet::None, |checker| {
            let (result, requests) = drive(
                &db,
                checker,
                instance(&db, candidate)?,
                member(&db, protocol, name)?,
                access,
                &OrdinaryDependencies,
            )?;
            assert_eq!(
                !result.is_never_satisfied(&db, checker.env),
                valid,
                "{candidate} as {protocol}"
            );
            assert_eq!(requests, steps);
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn nominal_read_retains_conditional_result() -> anyhow::Result<()> {
    let db = database()?;
    let required = member(&db, "GenericRead", "value")?;
    let env = db.program_environment();
    let variable = required
        .access(&db, &env, ProtocolMemberAccessMode::Instance)
        .read
        .and_then(|read| read.resolve(&db, &env))
        .and_then(|read| read.ty().as_typevar())
        .ok_or_else(|| anyhow::anyhow!("generic readable type should retain T"))?;
    with_checker(&db, TypeVarSet::from_typevars(&db, [variable]), |checker| {
        let int = instance(&db, "Data")?
            .member(&db, checker.env, "value")
            .place
            .expect_type();
        let expected = checker.check_type_pair(&db, int, Type::TypeVar(variable));
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        let (actual, _) = drive(
            &db,
            checker,
            instance(&db, "Data")?,
            required,
            ProtocolMemberAccessMode::Instance,
            &OrdinaryDependencies,
        )?;
        assert!(actual.ownership_probe_same_set(expected));
        Ok(())
    })
}

#[derive(Debug)]
struct Refused;

#[derive(Default)]
struct RefusingDependencies {
    calls: Cell<usize>,
    executed: Cell<usize>,
    refuse: Option<(usize, bool)>,
}

impl MemberPairDependencies for RefusingDependencies {
    type Error = Refused;
    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Refused> {
        let index = self.calls.get() + 1;
        self.calls.set(index);
        if self.refuse == Some((index, false)) {
            return Err(Refused);
        }
        let result = operation();
        self.executed.set(self.executed.get() + 1);
        if self.refuse == Some((index, true)) {
            return Err(Refused);
        }
        Ok(result)
    }
}

#[test]
fn nominal_read_refusal_drops_pending_work_and_retries() -> anyhow::Result<()> {
    let db = database()?;
    for (candidate, protocol, name, access) in [
        (
            "Data",
            "IntRead",
            "value",
            ProtocolMemberAccessMode::Instance,
        ),
        (
            "Data",
            "ObjectRead",
            "value",
            ProtocolMemberAccessMode::Instance,
        ),
        (
            "Good",
            "Method",
            "method",
            ProtocolMemberAccessMode::Instance,
        ),
        (
            "Alternatives",
            "Method",
            "method",
            ProtocolMemberAccessMode::Class,
        ),
    ] {
        with_checker(&db, TypeVarSet::None, |checker| {
            let ty = instance(&db, candidate)?;
            let member = member(&db, protocol, name)?;
            let dependencies = RefusingDependencies::default();
            let Ok((baseline, requests)) = drive(&db, checker, ty, member, access, &dependencies)
            else {
                anyhow::bail!("unrestricted finite read should complete");
            };
            for index in 1..=dependencies.calls.get() {
                for after in [false, true] {
                    let refusal = RefusingDependencies {
                        refuse: Some((index, after)),
                        ..RefusingDependencies::default()
                    };
                    assert!(drive(&db, checker, ty, member, access, &refusal).is_err());
                    assert_eq!(refusal.calls.get(), index);
                    assert_eq!(refusal.executed.get(), index - usize::from(!after));
                    for _ in 0..2 {
                        let (retry, retry_requests) =
                            drive(&db, checker, ty, member, access, &OrdinaryDependencies)?;
                        assert!(retry.ownership_probe_same_set(baseline));
                        assert_eq!(retry_requests, requests);
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
