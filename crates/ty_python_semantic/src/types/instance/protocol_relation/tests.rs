//! Finite controls for outer protocol continuations. Child operations run synchronously here;
//! the coordinated relation driver owns growing-chain and active-visit cancellation coverage.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{ProtocolRelationStep as Step, ordinary};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::relation::dependencies::{OrdinaryDependencies, RelationDependencies};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, ClassLiteral, ProtocolInstanceType, Type};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/outer_protocol.py",
            r#"from typing import ClassVar, Protocol

class ReadInt(Protocol):
    @property
    def value(self) -> int: ...

class Good:
    value: int

class Bad:
    value: str

class GenericMember[T](Protocol):
    value: T

class ReadObject(Protocol):
    @property
    def value(self) -> object: ...

class ClassSide(Protocol):
    flag: ClassVar[int]
    def method(self) -> int: ...

class ClassGood:
    flag: ClassVar[int]
    def method(self) -> int: ...

class ClassBad:
    flag: ClassVar[str]
    def method(self) -> int: ...
"#,
        )
        .build()
}

fn class_type<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/outer_protocol.py")?,
        env.program(db),
    );
    Ok(global_symbol(db, file, name).place.expect_type())
}

fn instance<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let class = class_type(db, name)?
        .as_class_literal()
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("missing fixture class {name}"))?;
    Ok(Type::instance(
        db,
        &db.program_environment(),
        class.identity_specialization(db),
    ))
}

fn protocol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ProtocolInstanceType<'db>> {
    instance(db, name)?
        .as_protocol_instance()
        .ok_or_else(|| anyhow::anyhow!("fixture {name} is not a protocol"))
}

fn with_checker<'db>(
    db: &'db TestDb,
    context: bool,
    inferable: TypeVarSet<'db>,
    check: impl FnOnce(&TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relations = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    let checker = if context {
        TypeRelationChecker::constraint_set_assignability_with_context(
            &env,
            &constraints,
            &relations,
            &disjointness,
            &signatures,
            &mapping,
        )
    } else {
        TypeRelationChecker::constraint_set_assignability(
            &env,
            &constraints,
            &relations,
            &disjointness,
            &signatures,
            &mapping,
        )
    }
    .with_inferable_typevars(inferable);
    check(&checker)
}

fn drive<'c, 'db, D: RelationDependencies>(
    db: &'db dyn Db,
    mut step: Step<'_, '_, 'c, 'db>,
    dependencies: &D,
) -> Result<(ConstraintSet<'db, 'c>, Vec<&'static str>), D::Error> {
    let mut requests = Vec::new();
    loop {
        step = match step {
            Step::Complete(result) => return Ok((result, requests)),
            Step::Relate(pending) => {
                requests.push("pair");
                let result = dependencies.run(db, || {
                    pending
                        .checker
                        .check_type_pair(db, pending.source, pending.target)
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::Interface(pending) => {
                requests.push("interface");
                let result = dependencies.run(db, || {
                    pending.checker.check_protocol_interface_pair(
                        db,
                        pending.source_type,
                        pending.source,
                        pending.target,
                    )
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::Member(pending) => {
                requests.push("member");
                let result = dependencies.run(db, || {
                    pending
                        .checker
                        .type_satisfies_protocol_member(db, pending.ty, &pending.member)
                })?;
                pending.resume(db, result, dependencies)?
            }
            Step::MetaBindings(pending) => {
                requests.push("bindings");
                let bindings = dependencies.run(db, || {
                    pending.constructor_ty.bindings(db, pending.checker.env)
                })?;
                pending.resume(db, &bindings, dependencies)?
            }
            Step::MetaMembers(pending) => {
                requests.push("meta-members");
                let result = dependencies.run(db, || {
                    pending.checker.check_meta_protocol_members(
                        db,
                        pending.instance_ty,
                        pending.meta_ty,
                        pending.protocol,
                    )
                })?;
                pending.resume(db, result, dependencies)?
            }
        };
    }
}

#[test]
fn finite_nominal_and_protocol_sources_preserve_results() -> anyhow::Result<()> {
    let db = database()?;
    let target = protocol(&db, "ReadInt")?;
    for context in [false, true] {
        for (source, accepts) in [
            ("Good", true),
            ("Bad", false),
            ("ReadInt", true),
            ("ReadObject", false),
        ] {
            let ty = instance(&db, source)?;
            with_checker(&db, context, TypeVarSet::None, |checker| {
                let (result, requests) = drive(
                    &db,
                    Step::start(&db, checker, ty, target, &OrdinaryDependencies)?,
                    &OrdinaryDependencies,
                )?;
                assert_eq!(
                    result.is_always_satisfied(&db, checker.env),
                    accepts,
                    "source={source}, context={context}"
                );
                let ordinary_result = ordinary(
                    &db,
                    Step::start(&db, checker, ty, target, &OrdinaryDependencies),
                );
                assert!(result.ownership_probe_same_set(ordinary_result));
                assert_eq!(requests.first(), Some(&"pair"));
                if source == "Good" || source == "Bad" {
                    assert!(requests.contains(&"member"));
                }
                if source == "ReadObject" {
                    assert!(requests.contains(&"interface"));
                }
                Ok(())
            })?;
        }
    }
    Ok(())
}

#[test]
fn finite_protocol_constraints_retain_original_builder() -> anyhow::Result<()> {
    let db = database()?;
    let target = protocol(&db, "GenericMember")?;
    let origin = target
        .class_origin(&db)
        .ok_or_else(|| anyhow::anyhow!("generic protocol origin"))?;
    let alias = origin
        .into_generic_alias()
        .ok_or_else(|| anyhow::anyhow!("generic protocol specialization"))?;
    let Some(Type::TypeVar(variable)) = alias.specialization(&db).types(&db).first().copied()
    else {
        anyhow::bail!("identity specialization should retain T");
    };
    with_checker(
        &db,
        false,
        TypeVarSet::from_typevars(&db, [variable]),
        |checker| {
            let (result, requests) = drive(
                &db,
                Step::start(
                    &db,
                    checker,
                    instance(&db, "Good")?,
                    target,
                    &OrdinaryDependencies,
                )?,
                &OrdinaryDependencies,
            )?;
            assert!(!result.is_trivially_always_satisfied());
            assert!(!result.is_trivially_never_satisfied());
            assert!(result.mentions_typevar(&db, variable));
            assert_eq!(requests, ["pair", "member"]);
            Ok(())
        },
    )
}

#[test]
fn meta_protocol_requests_bindings_before_instance_and_class_members() -> anyhow::Result<()> {
    let db = database()?;
    let target = protocol(&db, "ClassSide")?;
    for (source, accepts) in [("ClassGood", true), ("ClassBad", false)] {
        with_checker(&db, true, TypeVarSet::None, |checker| {
            let (result, requests) = drive(
                &db,
                Step::start_meta(
                    &db,
                    checker,
                    class_type(&db, source)?,
                    target,
                    &OrdinaryDependencies,
                )?,
                &OrdinaryDependencies,
            )?;
            assert_eq!(requests[0], "bindings");
            assert_eq!(requests[1], "pair");
            assert_eq!(result.is_always_satisfied(&db, checker.env), accepts);
            if accepts {
                assert_eq!(requests, ["bindings", "pair", "meta-members"]);
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[derive(Debug)]
struct Refused;

struct Boundaries {
    executed: Cell<usize>,
    refuse: usize,
    after: bool,
}

impl RelationDependencies for Boundaries {
    type Error = Refused;
    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Refused> {
        let current = self.executed.get() + 1;
        self.executed.set(current);
        if current == self.refuse && !self.after {
            return Err(Refused);
        }
        let result = operation();
        if current == self.refuse {
            return Err(Refused);
        }
        Ok(result)
    }
}

#[test]
fn refusal_stops_outer_continuations_before_the_next_dependency() -> anyhow::Result<()> {
    let db = database()?;
    let source = instance(&db, "Good")?;
    let target = protocol(&db, "ReadInt")?;
    with_checker(&db, true, TypeVarSet::None, |checker| {
        let baseline = Boundaries {
            executed: Cell::new(0),
            refuse: usize::MAX,
            after: false,
        };
        let step = Step::start(&db, checker, source, target, &baseline)
            .map_err(|_| anyhow::anyhow!("unexpected refusal"))?;
        let (expected, _) =
            drive(&db, step, &baseline).map_err(|_| anyhow::anyhow!("unexpected refusal"))?;
        for after in [false, true] {
            for refuse in 1..=baseline.executed.get() {
                let dependencies = Boundaries {
                    executed: Cell::new(0),
                    refuse,
                    after,
                };
                let outcome = Step::start(&db, checker, source, target, &dependencies)
                    .and_then(|step| drive(&db, step, &dependencies));
                assert!(outcome.is_err(), "boundary={refuse}, after={after}");
                assert_eq!(dependencies.executed.get(), refuse);
            }
        }
        for _ in 0..2 {
            let (result, _) = drive(
                &db,
                Step::start(&db, checker, source, target, &OrdinaryDependencies)?,
                &OrdinaryDependencies,
            )?;
            assert!(result.ownership_probe_same_set(expected));
        }
        Ok(())
    })
}
