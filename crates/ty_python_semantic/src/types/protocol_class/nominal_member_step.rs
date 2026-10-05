//! Instance/class and read/write continuations for a nominal protocol member comparison.

use super::relation_step::MemberPairDependencies;
use super::{
    ProtocolMember, ProtocolMemberAccess, ProtocolMemberAccessMode, ProtocolMemberType,
    ProtocolMemberWrite, ProtocolMemberWriteType, is_class_object_type,
};
use crate::Db;
use crate::types::constraints::ConstraintSet;
use crate::types::relation::TypeRelationChecker;
use crate::types::{ErrorContext, Type};

/// Inputs retained independently of the temporary protocol-member wrapper passed by a caller.
#[derive(Clone, Copy)]
pub(super) struct NominalMember<'checker, 'a, 'member, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(super) ty: Type<'db>,
    pub(super) member: ProtocolMember<'member, 'db>,
}

impl<'checker, 'a, 'member, 'c, 'db> NominalMember<'checker, 'a, 'member, 'c, 'db> {
    fn finish<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        if let Some(context) = self.checker.report_context()
            && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
        {
            context.push(ErrorContext::ProtocolMemberIncompatible {
                member_name: self.member.name.into(),
            });
        }
        Ok(ProtocolMemberStep::Complete(result))
    }
}

pub(super) enum ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Lookup(PendingAccessPresence<'checker, 'a, 'member, 'c, 'db>),
    Read(PendingAccessRead<'checker, 'a, 'member, 'c, 'db>),
    Write(PendingAccessWrite<'checker, 'a, 'member, 'c, 'db>),
}

impl<'checker, 'a, 'member, 'c, 'db> ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn start<D: MemberPairDependencies>(
        db: &'db dyn Db,
        input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
        receiver_ty: Type<'db>,
        required: ProtocolMemberAccess<'db>,
        access: ProtocolMemberAccessMode,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let NominalMember {
            checker,
            ty,
            member,
        } = input;
        if access == ProtocolMemberAccessMode::Class
            && dependencies.run(db, || {
                member.has_incompatible_class_variable_declaration(db, checker.env, ty)
            })?
        {
            if let Some(context) = checker.report_context() {
                context.push(ErrorContext::ProtocolMemberClassVarMismatch {
                    member_name: member.name.into(),
                    ty,
                });
            }
            return Ok(Self::Complete(checker.never()));
        }

        if access == ProtocolMemberAccessMode::Class
            && member.is_instance_method()
            && required.read.is_some()
        {
            // The instance-side check is authoritative for the signature of a method
            // implementation. Class access only establishes that the member is present. Callable
            // types and several callable literal forms do not expose a useful `__call__` member
            // through their meta-type.
            return Ok(if member.name == "__call__" {
                Self::Complete(checker.always())
            } else {
                Self::Lookup(PendingAccessPresence { input, receiver_ty })
            });
        }

        let required_write = required.write.map(ProtocolMemberWrite::compatibility_type);
        if let Some(required_ty) = required.read {
            Ok(Self::Read(PendingAccessRead {
                input,
                receiver_ty,
                required_ty,
                access,
                required_write,
            }))
        } else {
            AccessWrite {
                input,
                receiver_ty,
                required: required_write,
                access,
            }
            .start(db, checker.always(), dependencies)
        }
    }
}

pub(super) struct PendingAccessPresence<'checker, 'a, 'member, 'c, 'db> {
    pub(super) input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    pub(super) receiver_ty: Type<'db>,
}

impl<'checker, 'a, 'member, 'c, 'db> PendingAccessPresence<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn resume(
        self,
        result: Option<Type<'db>>,
    ) -> ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db> {
        ProtocolMemberAccessStep::Complete(ConstraintSet::from_bool(
            self.input.checker.constraints,
            result.is_some(),
        ))
    }
}

struct AccessWrite<'checker, 'a, 'member, 'c, 'db> {
    input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    receiver_ty: Type<'db>,
    required: Option<ProtocolMemberWriteType<'db>>,
    access: ProtocolMemberAccessMode,
}

impl<'checker, 'a, 'member, 'c, 'db> AccessWrite<'checker, 'a, 'member, 'c, 'db> {
    fn start<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        read_result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        let checker = self.input.checker;
        // ConstraintSet::and does not prepare the write when the read already rejects this type.
        if read_result.is_trivially_never_satisfied() {
            return Ok(ProtocolMemberAccessStep::Complete(read_result));
        }
        let Some(write) = self.required else {
            return dependencies
                .run(db, || {
                    read_result.and(db, checker.constraints, || checker.always())
                })
                .map(ProtocolMemberAccessStep::Complete);
        };
        let fallback_ty = dependencies.run(db, || {
            self.input
                .ty
                .literal_fallback_instance(db, checker.env)
                .unwrap_or(self.input.ty)
        })?;
        let receiver_ty = if self.access == ProtocolMemberAccessMode::Instance
            && matches!(self.input.ty, Type::LiteralValue(_))
        {
            fallback_ty
        } else {
            self.receiver_ty
        };
        let Some(value_ty) = dependencies.run(db, || write.bind(db, checker.env, fallback_ty))?
        else {
            return dependencies
                .run(db, || {
                    read_result.and(db, checker.constraints, || checker.never())
                })
                .map(ProtocolMemberAccessStep::Complete);
        };
        Ok(ProtocolMemberAccessStep::Write(PendingAccessWrite {
            checker,
            receiver_ty,
            member_name: self.input.member.name,
            value_ty,
            read_result,
        }))
    }
}

pub(super) struct PendingAccessRead<'checker, 'a, 'member, 'c, 'db> {
    pub(super) input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    pub(super) receiver_ty: Type<'db>,
    pub(super) required_ty: ProtocolMemberType<'db>,
    pub(super) access: ProtocolMemberAccessMode,
    required_write: Option<ProtocolMemberWriteType<'db>>,
}

impl<'checker, 'a, 'member, 'c, 'db> PendingAccessRead<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        AccessWrite {
            input: self.input,
            receiver_ty: self.receiver_ty,
            required: self.required_write,
            access: self.access,
        }
        .start(db, result, dependencies)
    }
}

pub(super) struct PendingAccessWrite<'checker, 'a, 'member, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    pub(super) receiver_ty: Type<'db>,
    pub(super) member_name: &'member str,
    pub(super) value_ty: Type<'db>,
    read_result: ConstraintSet<'db, 'c>,
}

impl<'checker, 'a, 'member, 'c, 'db> PendingAccessWrite<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberAccessStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        if let Some(context) = self.checker.report_context()
            && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
        {
            context.push(ErrorContext::ProtocolMemberWriteTypeIncompatible {
                target: self.value_ty,
            });
        }
        dependencies
            .run(db, || {
                self.read_result
                    .and(db, self.checker.constraints, || result)
            })
            .map(ProtocolMemberAccessStep::Complete)
    }
}

pub(super) enum ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Lookup(PendingMemberPresence<'checker, 'a, 'member, 'c, 'db>),
    Access(PendingMemberAccess<'checker, 'a, 'member, 'c, 'db>),
}

impl<'checker, 'a, 'member, 'c, 'db> ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn start<D: MemberPairDependencies>(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        member: &ProtocolMember<'member, 'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let instance_access = dependencies.run(db, || {
            member.implementation_access(db, checker.env, ty, ProtocolMemberAccessMode::Instance)
        })?;
        let input = NominalMember {
            checker,
            ty,
            member: *member,
        };
        if let Some(context) = checker.report_context() {
            if dependencies.run(db, || {
                member.has_incompatible_class_variable_declaration(db, checker.env, ty)
            })? {
                context.push(ErrorContext::ProtocolMemberClassVarMismatch {
                    member_name: member.name.into(),
                    ty,
                });
                context.push(ErrorContext::ProtocolMemberIncompatible {
                    member_name: member.name.into(),
                });
                return Ok(Self::Complete(checker.never()));
            }

            let preflight = MemberPreflight {
                input,
                instance_access,
            };
            return if instance_access.read.is_some() {
                Ok(Self::Lookup(PendingMemberPresence {
                    input,
                    receiver_ty: ty,
                    access: ProtocolMemberAccessMode::Instance,
                    continuation: PresenceContinuation::AfterInstance { instance_access },
                }))
            } else {
                preflight.after_instance(db, false, dependencies)
            };
        }
        Ok(Self::instance_access(input, instance_access))
    }

    fn instance_access(
        input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
        required: ProtocolMemberAccess<'db>,
    ) -> Self {
        Self::Access(PendingMemberAccess {
            input,
            receiver_ty: input.ty,
            required,
            access: ProtocolMemberAccessMode::Instance,
            continuation: AccessContinuation::AfterInstance,
        })
    }
}

struct MemberPreflight<'checker, 'a, 'member, 'c, 'db> {
    input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    instance_access: ProtocolMemberAccess<'db>,
}

impl<'checker, 'a, 'member, 'c, 'db> MemberPreflight<'checker, 'a, 'member, 'c, 'db> {
    fn after_instance<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        instance_read_missing: bool,
        dependencies: &D,
    ) -> Result<ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        let NominalMember {
            checker,
            ty,
            member,
        } = self.input;
        // Diagnostic preflight observes class access even if the instance lookup was missing.
        // The actual class comparison resolves it again after the instance comparison completes.
        let class_access = dependencies.run(db, || {
            member.implementation_access(db, checker.env, ty, ProtocolMemberAccessMode::Class)
        })?;
        if class_access.read.is_some()
            && !(member.is_instance_method() && member.name == "__call__")
        {
            let receiver_ty = dependencies.run(db, || ty.to_meta_type(db, checker.env))?;
            Ok(ProtocolMemberStep::Lookup(PendingMemberPresence {
                input: self.input,
                receiver_ty,
                access: ProtocolMemberAccessMode::Class,
                continuation: PresenceContinuation::AfterClass {
                    instance_access: self.instance_access,
                    instance_read_missing,
                },
            }))
        } else {
            Ok(self.after_class(instance_read_missing, false))
        }
    }

    fn after_class(
        self,
        instance_read_missing: bool,
        class_read_missing: bool,
    ) -> ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db> {
        let NominalMember {
            checker,
            ty,
            member,
        } = self.input;
        if instance_read_missing || class_read_missing {
            if let Some(context) = checker.report_context() {
                if instance_read_missing
                    && is_class_object_type(ty)
                    && member.is_instance_method()
                    && member.uses_special_method_lookup()
                {
                    context.push(ErrorContext::ProtocolSpecialMethodNotDefinedOnMetaType);
                }
                context.push(ErrorContext::ProtocolMemberNotDefined {
                    member_name: member.name.into(),
                    ty,
                });
            }
            ProtocolMemberStep::Complete(checker.never())
        } else {
            ProtocolMemberStep::instance_access(self.input, self.instance_access)
        }
    }
}

enum PresenceContinuation<'db> {
    AfterInstance {
        instance_access: ProtocolMemberAccess<'db>,
    },
    AfterClass {
        instance_access: ProtocolMemberAccess<'db>,
        instance_read_missing: bool,
    },
}

pub(super) struct PendingMemberPresence<'checker, 'a, 'member, 'c, 'db> {
    pub(super) input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    pub(super) receiver_ty: Type<'db>,
    pub(super) access: ProtocolMemberAccessMode,
    continuation: PresenceContinuation<'db>,
}

impl<'checker, 'a, 'member, 'c, 'db> PendingMemberPresence<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: Option<Type<'db>>,
        dependencies: &D,
    ) -> Result<ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        match self.continuation {
            PresenceContinuation::AfterInstance { instance_access } => MemberPreflight {
                input: self.input,
                instance_access,
            }
            .after_instance(db, result.is_none(), dependencies),
            PresenceContinuation::AfterClass {
                instance_access,
                instance_read_missing,
            } => Ok(MemberPreflight {
                input: self.input,
                instance_access,
            }
            .after_class(instance_read_missing, result.is_none())),
        }
    }
}

enum AccessContinuation<'db, 'c> {
    AfterInstance,
    AfterClass {
        instance_result: ConstraintSet<'db, 'c>,
    },
}

pub(super) struct PendingMemberAccess<'checker, 'a, 'member, 'c, 'db> {
    pub(super) input: NominalMember<'checker, 'a, 'member, 'c, 'db>,
    pub(super) receiver_ty: Type<'db>,
    pub(super) required: ProtocolMemberAccess<'db>,
    pub(super) access: ProtocolMemberAccessMode,
    continuation: AccessContinuation<'db, 'c>,
}

impl<'checker, 'a, 'member, 'c, 'db> PendingMemberAccess<'checker, 'a, 'member, 'c, 'db> {
    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberStep<'checker, 'a, 'member, 'c, 'db>, D::Error> {
        let NominalMember {
            checker,
            ty,
            member,
        } = self.input;
        match self.continuation {
            AccessContinuation::AfterInstance => {
                if result.is_trivially_never_satisfied() {
                    return self.input.finish(db, result, dependencies);
                }
                let required = dependencies.run(db, || {
                    member.implementation_access(
                        db,
                        checker.env,
                        ty,
                        ProtocolMemberAccessMode::Class,
                    )
                })?;
                let receiver_ty = dependencies.run(db, || ty.to_meta_type(db, checker.env))?;
                Ok(ProtocolMemberStep::Access(Self {
                    input: self.input,
                    receiver_ty,
                    required,
                    access: ProtocolMemberAccessMode::Class,
                    continuation: AccessContinuation::AfterClass {
                        instance_result: result,
                    },
                }))
            }
            AccessContinuation::AfterClass { instance_result } => {
                let result = dependencies.run(db, || {
                    instance_result.and(db, checker.constraints, || result)
                })?;
                self.input.finish(db, result, dependencies)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::convert::Infallible;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use ty_python_core::ProgramFile;

    use super::{
        MemberPairDependencies, NominalMember, ProtocolMemberAccessStep, ProtocolMemberStep,
    };
    use crate::Db;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::constraints::ConstraintSetBuilder;
    use crate::types::protocol_class::relation_step::OrdinaryDependencies;
    use crate::types::protocol_class::{ProtocolMemberAccessMode, protocol_member_read_type};
    use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
    use crate::types::signatures::SignatureRelationVisitor;
    use crate::types::{ApplyTypeMappingVisitor, ClassLiteral, MaterializationKind, Type};

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/nominal_member_steps.py",
                r#"from typing import Protocol

class Required(Protocol):
    value: int
    def method(self) -> int: ...

class Good:
    value: int
    def method(self) -> int: ...

class Bad:
    value: str
    def method(self) -> str: ...
"#,
            )
            .build()
    }

    fn instance<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
        let env = db.program_environment();
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/nominal_member_steps.py")?,
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

    fn with_checker<'db>(
        db: &'db TestDb,
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
        );
        check(&checker)
    }

    #[derive(Default)]
    struct CountingDependencies(Cell<usize>);

    impl MemberPairDependencies for CountingDependencies {
        type Error = Infallible;

        fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> Result<T, Infallible> {
            self.0.set(self.0.get() + 1);
            Ok(operation())
        }
    }

    #[test]
    fn nominal_access_retains_the_name_and_stops_before_write_preparation() -> anyhow::Result<()> {
        let db = database()?;
        let required = instance(&db, "Required")?
            .as_protocol_instance()
            .ok_or_else(|| anyhow::anyhow!("required fixture must be a protocol"))?;
        let candidate = instance(&db, "Bad")?;
        with_checker(&db, |checker| {
            let name = String::from("value");
            let step = {
                let mut member = required
                    .interface(&db)
                    .member_by_name(&db, &name)
                    .ok_or_else(|| anyhow::anyhow!("missing value member"))?;
                member.materialization = Some(MaterializationKind::Top);
                let access = member.implementation_access(
                    &db,
                    checker.env,
                    candidate,
                    ProtocolMemberAccessMode::Instance,
                );
                ProtocolMemberAccessStep::start(
                    &db,
                    NominalMember {
                        checker,
                        ty: candidate,
                        member,
                    },
                    candidate,
                    access,
                    ProtocolMemberAccessMode::Instance,
                    &OrdinaryDependencies,
                )?
            };
            let ProtocolMemberAccessStep::Read(pending) = step else {
                anyhow::bail!("instance access begins with a read");
            };
            assert!(std::ptr::eq(pending.input.checker, checker));
            assert_eq!(pending.input.member.name.as_ptr(), name.as_ptr());
            assert_eq!(
                pending.input.member.materialization,
                Some(MaterializationKind::Top)
            );
            let result = checker.check_protocol_member_read(
                &db,
                candidate,
                pending.receiver_ty,
                &pending.input.member,
                pending.required_ty,
                pending.access,
            );
            assert!(result.is_trivially_never_satisfied());
            let dependencies = CountingDependencies::default();
            let ProtocolMemberAccessStep::Complete(completed) =
                pending.resume(&db, result, &dependencies)?
            else {
                anyhow::bail!("a negative read skips write preparation");
            };
            assert!(completed.ownership_probe_same_set(result));
            assert_eq!(dependencies.0.get(), 0);
            Ok(())
        })
    }

    #[test]
    fn nominal_context_preflight_precedes_access_and_repeats_class_preparation()
    -> anyhow::Result<()> {
        let db = database()?;
        let required = instance(&db, "Required")?
            .as_protocol_instance()
            .ok_or_else(|| anyhow::anyhow!("required fixture must be a protocol"))?;
        for candidate_name in ["Good", "Bad"] {
            let candidate = instance(&db, candidate_name)?;
            with_checker(&db, |checker| {
                let name = String::from("method");
                let dependencies = CountingDependencies::default();
                let step = {
                    let member = required
                        .interface(&db)
                        .member_by_name(&db, &name)
                        .ok_or_else(|| anyhow::anyhow!("missing method member"))?;
                    ProtocolMemberStep::start(&db, checker, candidate, &member, &dependencies)?
                };
                assert_eq!(dependencies.0.get(), 2);
                let ProtocolMemberStep::Lookup(instance_lookup) = step else {
                    anyhow::bail!("context preflight first checks instance presence");
                };
                assert!(instance_lookup.access == ProtocolMemberAccessMode::Instance);
                assert_eq!(instance_lookup.input.member.name.as_ptr(), name.as_ptr());
                let result = protocol_member_read_type(
                    &db,
                    checker.env,
                    candidate,
                    instance_lookup.receiver_ty,
                    &instance_lookup.input.member,
                    instance_lookup.access,
                );
                assert!(result.is_some());
                let ProtocolMemberStep::Lookup(class_lookup) =
                    instance_lookup.resume(&db, result, &dependencies)?
                else {
                    anyhow::bail!("class presence is checked before comparing instance access");
                };
                assert!(class_lookup.access == ProtocolMemberAccessMode::Class);
                assert_eq!(dependencies.0.get(), 4);
                let result = protocol_member_read_type(
                    &db,
                    checker.env,
                    candidate,
                    class_lookup.receiver_ty,
                    &class_lookup.input.member,
                    class_lookup.access,
                );
                assert!(result.is_some());
                let ProtocolMemberStep::Access(instance_access) =
                    class_lookup.resume(&db, result, &dependencies)?
                else {
                    anyhow::bail!("successful preflight leads to instance access");
                };
                assert!(instance_access.access == ProtocolMemberAccessMode::Instance);
                assert_eq!(dependencies.0.get(), 4);
                let result = checker.type_satisfies_protocol_member_access(
                    &db,
                    candidate,
                    instance_access.receiver_ty,
                    &instance_access.input.member,
                    instance_access.required,
                    instance_access.access,
                );
                let step = instance_access.resume(&db, result, &dependencies)?;
                if candidate_name == "Bad" {
                    let ProtocolMemberStep::Complete(completed) = step else {
                        anyhow::bail!("a negative instance result skips the class comparison");
                    };
                    assert!(completed.is_trivially_never_satisfied());
                    assert!(completed.ownership_probe_same_set(result));
                    // Only the enclosing incompatibility diagnostic is prepared after rejection.
                    assert_eq!(dependencies.0.get(), 5);
                } else {
                    let ProtocolMemberStep::Access(class_access) = step else {
                        anyhow::bail!("a successful instance result proceeds to class access");
                    };
                    assert!(class_access.access == ProtocolMemberAccessMode::Class);
                    // Class access and the meta-type are obtained again after instance completion.
                    assert_eq!(dependencies.0.get(), 6);
                    let result = checker.type_satisfies_protocol_member_access(
                        &db,
                        candidate,
                        class_access.receiver_ty,
                        &class_access.input.member,
                        class_access.required,
                        class_access.access,
                    );
                    let ProtocolMemberStep::Complete(completed) =
                        class_access.resume(&db, result, &dependencies)?
                    else {
                        anyhow::bail!("the class result completes the member comparison");
                    };
                    assert!(completed.is_trivially_always_satisfied());
                    assert_eq!(dependencies.0.get(), 8);
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    #[test]
    fn nominal_pending_write_can_be_dropped_and_retried() -> anyhow::Result<()> {
        let db = database()?;
        let required = instance(&db, "Required")?
            .as_protocol_instance()
            .ok_or_else(|| anyhow::anyhow!("required fixture must be a protocol"))?;
        let candidate = instance(&db, "Good")?;
        with_checker(&db, |checker| {
            let name = String::from("value");
            let member = required
                .interface(&db)
                .member_by_name(&db, &name)
                .ok_or_else(|| anyhow::anyhow!("missing value member"))?;
            let required_access = member.implementation_access(
                &db,
                checker.env,
                candidate,
                ProtocolMemberAccessMode::Instance,
            );
            let context = checker
                .report_context()
                .ok_or_else(|| anyhow::anyhow!("context collection is enabled"))?;
            let initial_context = context.snapshot();
            for abandon in [true, false, false] {
                let ProtocolMemberAccessStep::Read(read) = ProtocolMemberAccessStep::start(
                    &db,
                    NominalMember {
                        checker,
                        ty: candidate,
                        member,
                    },
                    candidate,
                    required_access,
                    ProtocolMemberAccessMode::Instance,
                    &OrdinaryDependencies,
                )?
                else {
                    anyhow::bail!("mutable instance access begins with a read");
                };
                let result = checker.check_protocol_member_read(
                    &db,
                    candidate,
                    read.receiver_ty,
                    &read.input.member,
                    read.required_ty,
                    read.access,
                );
                let ProtocolMemberAccessStep::Write(write) =
                    read.resume(&db, result, &OrdinaryDependencies)?
                else {
                    anyhow::bail!("a successful read proceeds to the write");
                };
                assert!(std::ptr::eq(write.checker, checker));
                assert_eq!(write.member_name.as_ptr(), name.as_ptr());
                if !abandon {
                    let result = write.checker.check_attribute_write(
                        &db,
                        write.receiver_ty,
                        write.member_name,
                        write.value_ty,
                    );
                    let ProtocolMemberAccessStep::Complete(completed) =
                        write.resume(&db, result, &OrdinaryDependencies)?
                    else {
                        anyhow::bail!("write completion finishes the access comparison");
                    };
                    assert!(completed.is_trivially_always_satisfied());
                }
                assert_eq!(context.snapshot(), initial_context);
            }
            Ok(())
        })
    }
}
