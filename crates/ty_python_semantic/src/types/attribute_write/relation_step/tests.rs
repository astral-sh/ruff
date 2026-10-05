//! Finite controls for shared write continuations; callbacks use ordinary semantic evaluation.

use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ty_python_core::ProgramFile;

use super::{AttributeWriteStep as Step, WriteInput, WriteOperation, evaluate};
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::attribute_write::{
    AttributeWriteRequirement, ClassAttributeWriteMember, ExplicitAttributeWriteRequirement,
    FallbackAttributeWriteRequirement, InstanceAttributeWriteMember, attribute_write_requirement,
};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::relation::dependencies::{OrdinaryDependencies, RelationDependencies};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, IntersectionType, KnownClass, Type, TypeQualifiers,
    UnionType,
};

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/write_steps.py",
            r#"from dataclasses import dataclass
from typing import ClassVar, Final, Generic as LegacyGeneric, Never, TypeVar, overload

class Data:
    value: int

class ClassValue:
    value: ClassVar[int] = 1

class FinalValue:
    value: Final[int] = 1

class Property:
    @property
    def value(self) -> int: ...
    @value.setter
    def value(self, value: int) -> None: ...

class ReadOnly:
    @property
    def value(self) -> int: ...

class Descriptor:
    def __set__(self, instance: object, value: int) -> None: ...

class DescriptorOwner:
    value = Descriptor()

class Sink:
    def __setattr__(self, name: str, value: int) -> None: ...

class Blocked:
    value: int
    def __setattr__(self, name: str, value: object) -> Never: ...

@dataclass(frozen=True)
class Frozen:
    value: int

class Generic[T]:
    value: T
    @overload
    def put(self, value: int) -> None: ...
    @overload
    def put(self, value: T) -> None: ...
    def put(self, value: object) -> None: ...

U = TypeVar("U")
class Legacy(LegacyGeneric[U]):
    value: U
    @overload
    def put(self, value: int) -> None: ...
    @overload
    def put(self, value: U) -> None: ...
    def put(self, value: object) -> None: ...
"#,
        )
        .build()
}

fn class_type<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/write_steps.py")?,
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

#[test]
fn write_steps_cover_declarations_properties_descriptors_and_setattr() -> anyhow::Result<()> {
    let db = database()?;
    let int_value = Type::int_literal(1);
    let str_value = Type::string_literal(&db, "value");
    for (name, class_access, value, valid) in [
        ("Data", false, int_value, true),
        ("Data", false, str_value, false),
        ("ClassValue", false, int_value, false),
        ("ClassValue", true, int_value, true),
        ("ClassValue", true, str_value, false),
        ("FinalValue", false, int_value, false),
        ("Property", false, int_value, true),
        ("Property", false, str_value, false),
        ("ReadOnly", false, int_value, false),
        ("DescriptorOwner", false, int_value, true),
        ("DescriptorOwner", false, str_value, false),
        ("Sink", false, int_value, true),
        ("Sink", false, str_value, false),
        ("Blocked", false, int_value, false),
        ("Frozen", false, int_value, false),
    ] {
        let object = if class_access {
            class_type(&db, name)?
        } else {
            instance(&db, name)?
        };
        with_checker(&db, TypeVarSet::None, |checker| {
            let result = evaluate(
                &db,
                WriteInput {
                    checker,
                    member_name: "value",
                    value_ty: value,
                },
                WriteOperation::Resolve(object),
                &OrdinaryDependencies,
            )?;
            assert_eq!(result.is_trivially_always_satisfied(), valid, "{name}");
            assert_eq!(result.is_trivially_never_satisfied(), !valid, "{name}");
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn write_steps_invoke_setters_before_selecting_the_member_or_reading_the_setter()
-> anyhow::Result<()> {
    let db = database()?;
    let object = instance(&db, "DescriptorOwner")?;
    with_checker(&db, TypeVarSet::None, |checker| {
        let name = String::from("value");
        let input = WriteInput {
            checker,
            member_name: &name,
            value_ty: Type::int_literal(1),
        };
        let requirement = attribute_write_requirement(&db, checker.env, object, &name);
        let Step::Invoke(setattr) = Step::start(
            &db,
            input,
            WriteOperation::Requirement(requirement),
            &OrdinaryDependencies,
        )?
        else {
            anyhow::bail!("instance writes invoke __setattr__ before selecting the member");
        };
        assert_eq!(setattr.request.name, "__setattr__");
        assert!(std::ptr::eq(setattr.input.checker, checker));
        assert_eq!(setattr.input.member_name.as_ptr(), name.as_ptr());
        let result = setattr
            .request
            .evaluate(&db, checker.env, &setattr.arguments);
        let Step::Evaluate(explicit) = setattr.resume(&db, &result, &OrdinaryDependencies)? else {
            anyhow::bail!("the descriptor member is selected after __setattr__ completes");
        };
        let Step::Invoke(setter) = Step::start(
            &db,
            explicit.input,
            explicit.operation,
            &OrdinaryDependencies,
        )?
        else {
            anyhow::bail!("descriptor writes probe __set__ before inspecting its value parameter");
        };
        assert_eq!(setter.request.name, "__set__");
        let result = setter.request.evaluate(&db, checker.env, &setter.arguments);
        assert!(result.is_ok());
        let Step::Lookup(lookup) = setter.resume(&db, &result, &OrdinaryDependencies)? else {
            anyhow::bail!("setter inspection follows the completed probe");
        };
        assert!(matches!(
            lookup.lookup,
            super::WriteLookup::DescriptorSetter(_)
        ));
        // Abandon a pending lookup, then re-evaluate against the same checker and revision.
        for _ in 0..2 {
            let result = evaluate(
                &db,
                input,
                WriteOperation::Resolve(object),
                &OrdinaryDependencies,
            )?;
            assert!(result.is_trivially_always_satisfied());
        }
        Ok(())
    })
}

#[test]
fn write_steps_preserve_instance_and_class_fallback_asymmetry() -> anyhow::Result<()> {
    let db = database()?;
    let object = instance(&db, "Data")?;
    with_checker(&db, TypeVarSet::None, |checker| {
        let int_ty = KnownClass::Int.to_instance(&db, checker.env);
        let str_ty = KnownClass::Str.to_instance(&db, checker.env);
        for class_access in [false, true] {
            let member = ExplicitAttributeWriteRequirement::AssignableTo {
                ty: int_ty,
                qualifiers: TypeQualifiers::default(),
            };
            let fallback = Some(FallbackAttributeWriteRequirement::AssignableTo {
                ty: str_ty,
                qualifiers: TypeQualifiers::default(),
                possibly_missing: false,
            });
            let requirement = if class_access {
                AttributeWriteRequirement::Class {
                    object_ty: object,
                    member: ClassAttributeWriteMember::Explicit { member, fallback },
                }
            } else {
                AttributeWriteRequirement::Instance {
                    object_ty: object,
                    member: InstanceAttributeWriteMember::Explicit { member, fallback },
                }
            };
            let input = WriteInput {
                checker,
                member_name: "value",
                value_ty: str_ty,
            };
            let mut step = Step::start(
                &db,
                input,
                WriteOperation::Requirement(requirement),
                &OrdinaryDependencies,
            )?;
            if let Step::Invoke(pending) = step {
                let result = pending
                    .request
                    .evaluate(&db, checker.env, &pending.arguments);
                step = pending.resume(&db, &result, &OrdinaryDependencies)?;
            }
            let Step::Evaluate(explicit) = step else {
                anyhow::bail!("explicit write expected");
            };
            let result = evaluate(
                &db,
                explicit.input,
                explicit.operation,
                &OrdinaryDependencies,
            )?;
            assert!(result.is_trivially_never_satisfied());
            let step = explicit
                .continuation
                .resume(&db, result, &OrdinaryDependencies)?;
            if class_access {
                assert!(
                    matches!(step, Step::Complete(result) if result.is_trivially_never_satisfied())
                );
            } else {
                let Step::Evaluate(fallback) = step else {
                    anyhow::bail!("instance writes still evaluate the fallback after rejection");
                };
                assert!(matches!(fallback.operation, WriteOperation::Fallback(_)));
                let fallback_result = evaluate(
                    &db,
                    fallback.input,
                    fallback.operation,
                    &OrdinaryDependencies,
                )?;
                assert!(fallback_result.is_trivially_always_satisfied());
                assert!(
                    matches!(fallback.continuation.resume(&db, fallback_result, &OrdinaryDependencies)?, Step::Complete(completed) if completed.ownership_probe_same_set(result))
                );
            }
        }
        Ok(())
    })
}

#[test]
fn callable_write_folds_retain_conditional_constraints_in_both_generic_syntaxes()
-> anyhow::Result<()> {
    let db = database()?;
    for name in ["Generic", "Legacy"] {
        let object = instance(&db, name)?;
        let env = db.program_environment();
        let Type::TypeVar(variable) = object.member(&db, &env, "value").place.expect_type() else {
            anyhow::bail!("generic declaration must retain its variable");
        };
        let callable_ty = object.member(&db, &env, "put").place.expect_type();
        with_checker(&db, TypeVarSet::from_typevars(&db, [variable]), |checker| {
            let value_ty = KnownClass::Str.to_instance(&db, checker.env);
            let expected = checker.check_type_pair(&db, value_ty, Type::TypeVar(variable));
            assert!(!expected.is_trivially_always_satisfied());
            assert!(!expected.is_trivially_never_satisfied());
            let result = evaluate(
                &db,
                WriteInput {
                    checker,
                    member_name: "value",
                    value_ty,
                },
                WriteOperation::CallableParameter {
                    callable_ty,
                    parameter_index: 0,
                    self_ty: object,
                },
                &OrdinaryDependencies,
            )?;
            assert!(result.ownership_probe_same_set(expected));
            Ok(())
        })?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Phase {
    Before,
    After,
}

#[test]
fn sequential_write_requirements_stop_before_resolving_the_next_member() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let union = UnionType::from_elements(
        &db,
        &env,
        [instance(&db, "ReadOnly")?, instance(&db, "Blocked")?],
    );
    let Type::Union(union) = union else {
        anyhow::bail!("two distinct receivers are required");
    };
    let intersection = IntersectionType::new(
        &db,
        [instance(&db, "Data")?, instance(&db, "Sink")?]
            .into_iter()
            .collect::<crate::FxOrderSet<_>>(),
        crate::types::set_theoretic::NegativeIntersectionElements::default(),
    );
    with_checker(&db, TypeVarSet::None, |checker| {
        for all in [true, false] {
            let requirement = if all {
                AttributeWriteRequirement::All {
                    object_ty: Type::Union(union),
                    element_tys: union.elements(&db),
                }
            } else {
                AttributeWriteRequirement::Any {
                    object_ty: Type::Intersection(intersection),
                    intersection,
                }
            };
            let input = WriteInput {
                checker,
                member_name: "value",
                value_ty: Type::int_literal(1),
            };
            let Step::Evaluate(first) = Step::start(
                &db,
                input,
                WriteOperation::Requirement(requirement),
                &OrdinaryDependencies,
            )?
            else {
                anyhow::bail!("the first receiver must be evaluated");
            };
            assert!(matches!(first.operation, WriteOperation::Resolve(_)));
            let result = evaluate(&db, first.input, first.operation, &OrdinaryDependencies)?;
            let dependencies = Dependencies::default();
            let Ok(Step::Complete(completed)) =
                first.continuation.resume(&db, result, &dependencies)
            else {
                anyhow::bail!("the absorbing result must stop before the next resolution");
            };
            assert_eq!(completed.is_trivially_never_satisfied(), all);
            assert_eq!(completed.is_trivially_always_satisfied(), !all);
            assert_eq!(dependencies.calls.get(), 1);
        }
        Ok(())
    })
}

#[derive(Debug)]
struct Refused;

#[derive(Default)]
struct Dependencies {
    refuse: Option<(usize, Phase)>,
    calls: Cell<usize>,
    executed: Cell<usize>,
}

impl RelationDependencies for Dependencies {
    type Error = Refused;

    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Self::Error> {
        let index = self.calls.get() + 1;
        self.calls.set(index);
        if matches!(self.refuse, Some((at, Phase::Before)) if at == index) {
            return Err(Refused);
        }
        let result = operation();
        self.executed.set(self.executed.get() + 1);
        if matches!(self.refuse, Some((at, Phase::After)) if at == index) {
            return Err(Refused);
        }
        Ok(result)
    }
}

#[test]
fn finite_write_callbacks_refuse_before_consumption_and_allow_unchanged_retries()
-> anyhow::Result<()> {
    let db = database()?;
    for name in ["DescriptorOwner", "Sink"] {
        let object = instance(&db, name)?;
        with_checker(&db, TypeVarSet::None, |checker| {
            let input = WriteInput {
                checker,
                member_name: "value",
                value_ty: Type::int_literal(1),
            };
            let baseline_dependencies = Dependencies::default();
            let Ok(baseline) = evaluate(
                &db,
                input,
                WriteOperation::Resolve(object),
                &baseline_dependencies,
            ) else {
                anyhow::bail!("finite baseline must complete");
            };
            assert!(baseline.is_trivially_always_satisfied());
            for index in 1..=baseline_dependencies.calls.get() {
                for phase in [Phase::Before, Phase::After] {
                    let dependencies = Dependencies {
                        refuse: Some((index, phase)),
                        ..Dependencies::default()
                    };
                    assert!(matches!(
                        evaluate(&db, input, WriteOperation::Resolve(object), &dependencies),
                        Err(Refused)
                    ));
                    assert_eq!(dependencies.calls.get(), index);
                    assert_eq!(
                        dependencies.executed.get(),
                        match phase {
                            Phase::Before => index - 1,
                            Phase::After => index,
                        }
                    );
                    for _ in 0..2 {
                        let retry = evaluate(
                            &db,
                            input,
                            WriteOperation::Resolve(object),
                            &OrdinaryDependencies,
                        )?;
                        assert!(retry.ownership_probe_same_set(baseline));
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
