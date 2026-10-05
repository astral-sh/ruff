//! Readable protocol-member requirements, with recursive dependencies returned to the caller.

use std::ops::ControlFlow;

use smallvec::SmallVec;

use super::nominal_member_step::NominalMember;
use super::relation_step::MemberPairDependencies;
use super::{
    ProtocolMember, ProtocolMemberAccessMode, ProtocolMemberType,
    protocol_apply_self_with_receiver, protocol_bind_self,
};
use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
#[cfg(test)]
use crate::types::constructor::expansion_probe::descriptor_observation;
use crate::types::relation::TypeRelationChecker;
use crate::types::{CallableSignature, CallableType, CallableTypes, ErrorContext, Type};

#[cfg(test)]
mod tests;

pub(super) enum ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Presence(PendingReadPresence<'checker, 'state, 'member, 'c, 'db>),
    Lookup(PendingReadLookup<'checker, 'state, 'member, 'c, 'db>),
    Convert(PendingReadConversion<'checker, 'state, 'c, 'db>),
    Relate(PendingReadRelation<'checker, 'state, 'c, 'db>),
    Callables(PendingReadCallables<'checker, 'state, 'c, 'db>),
    Callable(PendingReadCallable<'checker, 'state, 'c, 'db>),
}

struct ReadInput<'checker, 'state, 'member, 'c, 'db> {
    checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    ty: Type<'db>,
    receiver_ty: Type<'db>,
    member: ProtocolMember<'member, 'db>,
    required_ty: ProtocolMemberType<'db>,
    access: ProtocolMemberAccessMode,
}

impl<'checker, 'state, 'member, 'c, 'db>
    ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>
{
    pub(super) fn start<D: MemberPairDependencies>(
        db: &'db dyn Db,
        input: NominalMember<'checker, 'state, 'member, 'c, 'db>,
        receiver_ty: Type<'db>,
        required_ty: ProtocolMemberType<'db>,
        access: ProtocolMemberAccessMode,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let NominalMember {
            checker,
            ty,
            member,
        } = input;
        let input = ReadInput {
            checker,
            ty,
            receiver_ty,
            member,
            required_ty,
            access,
        };
        // Reading a member as `object` imposes no constraint on its value type. A class
        // attribute establishes presence without inferring a shadowing instance assignment.
        if !member.is_method()
            && let Some(required) = dependencies.run(db, || required_ty.resolve(db, checker.env))?
            && dependencies.run(db, || required.ty().resolve_type_alias(db))? == Type::object()
        {
            return Ok(Self::Presence(PendingReadPresence { input }));
        }
        Ok(Self::Lookup(PendingReadLookup { input }))
    }
}

pub(super) struct PendingReadPresence<'checker, 'state, 'member, 'c, 'db> {
    input: ReadInput<'checker, 'state, 'member, 'c, 'db>,
}

impl<'checker, 'state, 'member, 'c, 'db> PendingReadPresence<'checker, 'state, 'member, 'c, 'db> {
    pub(super) fn receiver(&self) -> Type<'db> {
        self.input.receiver_ty
    }
    pub(super) fn name(&self) -> &'member str {
        self.input.member.name
    }

    pub(super) fn resume(
        self,
        present: bool,
    ) -> ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db> {
        if present {
            ProtocolMemberReadStep::Complete(self.input.checker.always())
        } else {
            ProtocolMemberReadStep::Lookup(PendingReadLookup { input: self.input })
        }
    }
}

pub(super) struct PendingReadLookup<'checker, 'state, 'member, 'c, 'db> {
    input: ReadInput<'checker, 'state, 'member, 'c, 'db>,
}

impl<'checker, 'state, 'member, 'c, 'db> PendingReadLookup<'checker, 'state, 'member, 'c, 'db> {
    pub(super) fn candidate(&self) -> Type<'db> {
        self.input.ty
    }
    pub(super) fn receiver(&self) -> Type<'db> {
        self.input.receiver_ty
    }
    pub(super) fn member(&self) -> &ProtocolMember<'member, 'db> {
        &self.input.member
    }
    pub(super) fn access(&self) -> ProtocolMemberAccessMode {
        self.input.access
    }

    pub(super) fn resume<D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        attribute_type: Option<Type<'db>>,
        dependencies: &D,
    ) -> Result<ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>, D::Error> {
        let ReadInput {
            checker,
            ty,
            receiver_ty,
            member,
            required_ty,
            access,
        } = self.input;
        let Some(attribute_type) = attribute_type else {
            return Ok(ProtocolMemberReadStep::Complete(checker.never()));
        };
        let env = checker.env;
        // `Self` in a protocol member names the value satisfying the protocol. `Self` in a
        // method on a class object names instances of that class: a `@classmethod` returning
        // `Self` returns `Factory`, not `type[Factory]`. Keep the bindings separate so a method
        // that returns an instance cannot satisfy a protocol that promises the class object.
        let protocol_self =
            dependencies.run(db, || ty.literal_fallback_instance(db, env).unwrap_or(ty))?;
        let implementation_self = if let Some(instance) =
            dependencies.run(db, || ty.to_instance_approximation(db, env))?
        {
            instance
        } else {
            dependencies
                .run(db, || ty.literal_fallback_instance(db, env))?
                .unwrap_or(ty)
        };
        let (implementation_receiver, protocol_receiver) = if member.is_class_method() {
            (
                dependencies.run(db, || implementation_self.to_meta_type(db, env))?,
                dependencies.run(db, || protocol_self.to_meta_type(db, env))?,
            )
        } else {
            (implementation_self, protocol_self)
        };

        if member.is_method() {
            let Some(required) = dependencies.run(db, || required_ty.resolve(db, env))? else {
                return Ok(ProtocolMemberReadStep::Complete(checker.never()));
            };
            let Type::Callable(required_callable) = required.ty() else {
                return Ok(ProtocolMemberReadStep::Complete(checker.never()));
            };
            if access == ProtocolMemberAccessMode::Instance || member.is_instance_method() {
                return Ok(ProtocolMemberReadStep::Convert(PendingReadConversion {
                    checker,
                    source: attribute_type,
                    required: required_callable,
                    implementation_self,
                    protocol_self,
                    binding: if access == ProtocolMemberAccessMode::Instance {
                        MethodBinding::Instance {
                            implementation_receiver,
                            protocol_receiver,
                        }
                    } else {
                        MethodBinding::Class
                    },
                }));
            }
            let target = dependencies.run(db, || {
                protocol_apply_self_with_receiver(
                    db,
                    env.program(db),
                    required_callable,
                    protocol_receiver,
                    protocol_self,
                )
            })?;
            return Ok(ProtocolMemberReadStep::Relate(PendingReadRelation {
                checker,
                source: attribute_type,
                target: Type::Callable(target),
                report_read: false,
            }));
        }

        let Some(target) =
            dependencies.run(db, || required_ty.bind_self(db, env, protocol_self))?
        else {
            return Ok(ProtocolMemberReadStep::Complete(checker.never()));
        };
        #[cfg(test)]
        if descriptor_observation::within_descriptor() {
            descriptor_observation::event(
                "ProtocolChild",
                (
                    member.name,
                    std::ptr::from_ref(checker).addr(),
                    descriptor_observation::key(ty),
                    descriptor_observation::key(receiver_ty),
                    descriptor_observation::key(attribute_type),
                    descriptor_observation::key(target),
                    crate::types::constructor::expansion_probe::stopped(db),
                ),
            );
        }
        #[cfg(not(test))]
        let _ = receiver_ty;
        Ok(ProtocolMemberReadStep::Relate(PendingReadRelation {
            checker,
            source: attribute_type,
            target,
            report_read: true,
        }))
    }
}

enum MethodBinding<'db> {
    Instance {
        implementation_receiver: Type<'db>,
        protocol_receiver: Type<'db>,
    },
    Class,
}

pub(super) struct PendingReadConversion<'checker, 'state, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    pub(super) source: Type<'db>,
    required: CallableType<'db>,
    implementation_self: Type<'db>,
    protocol_self: Type<'db>,
    binding: MethodBinding<'db>,
}

impl<'checker, 'state, 'c, 'db> PendingReadConversion<'checker, 'state, 'c, 'db> {
    pub(super) fn resume<'member, D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        callables: Option<CallableTypes<'db>>,
        dependencies: &D,
    ) -> Result<ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>, D::Error> {
        let Some(callables) = callables else {
            return Ok(ProtocolMemberReadStep::Complete(self.checker.never()));
        };
        match self.binding {
            MethodBinding::Instance {
                implementation_receiver,
                protocol_receiver,
            } => {
                let mut source = SmallVec::with_capacity(callables.iter().len());
                for callable in &callables {
                    source.push(dependencies.run(db, || {
                        protocol_apply_self_with_receiver(
                            db,
                            self.checker.env.program(db),
                            *callable,
                            implementation_receiver,
                            self.implementation_self,
                        )
                    })?);
                }
                let source = dependencies.run(db, || CallableTypes::new(source))?;
                let target = dependencies.run(db, || {
                    protocol_apply_self_with_receiver(
                        db,
                        self.checker.env.program(db),
                        self.required,
                        protocol_receiver,
                        self.protocol_self,
                    )
                })?;
                Ok(ProtocolMemberReadStep::Callables(PendingReadCallables {
                    checker: self.checker,
                    source,
                    target,
                }))
            }
            MethodBinding::Class => ClassMethodAlternatives {
                checker: self.checker,
                callables,
                next: 0,
                required: self.required,
                implementation_self: self.implementation_self,
                protocol_self: self.protocol_self,
                fold: ConstraintFold::new(self.checker.constraints, ConstraintFoldKind::All),
            }
            .next(db, dependencies),
        }
    }
}

pub(super) struct PendingReadRelation<'checker, 'state, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    pub(super) source: Type<'db>,
    pub(super) target: Type<'db>,
    report_read: bool,
}

impl<'checker, 'state, 'c, 'db> PendingReadRelation<'checker, 'state, 'c, 'db> {
    pub(super) fn resume<'member, D: MemberPairDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>, D::Error> {
        if self.report_read
            && let Some(context) = self.checker.report_context()
            && dependencies.run(db, || result.is_never_satisfied(db, self.checker.env))?
        {
            context.push(ErrorContext::ProtocolMemberReadTypeIncompatible {
                source: self.source,
                target: self.target,
            });
        }
        Ok(ProtocolMemberReadStep::Complete(result))
    }
}

pub(super) struct PendingReadCallables<'checker, 'state, 'c, 'db> {
    pub(super) checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    pub(super) source: CallableTypes<'db>,
    pub(super) target: CallableType<'db>,
}

struct ClassMethodAlternatives<'checker, 'state, 'c, 'db> {
    checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    callables: CallableTypes<'db>,
    next: usize,
    required: CallableType<'db>,
    implementation_self: Type<'db>,
    protocol_self: Type<'db>,
    fold: ConstraintFold<'db, 'c>,
}

impl<'checker, 'state, 'c, 'db> ClassMethodAlternatives<'checker, 'state, 'c, 'db> {
    fn next<'member, D: MemberPairDependencies>(
        mut self,
        db: &'db dyn Db,
        dependencies: &D,
    ) -> Result<ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>, D::Error> {
        let Some(callable) = self.callables.iter().as_slice().get(self.next).copied() else {
            return dependencies
                .run(db, || self.fold.finish())
                .map(ProtocolMemberReadStep::Complete);
        };
        self.next += 1;
        let (source, target) = if dependencies.run(db, || callable.is_function_like(db))? {
            // Require a positional receiver before binding: a zero-argument static
            // method otherwise loses no parameters while the protocol loses `self`.
            let signatures = dependencies.run(db, || callable.signatures(db))?;
            let overloads = signatures
                .iter()
                .filter(|signature| {
                    let parameters = signature.parameters();
                    parameters.get_positional(0).is_some() || parameters.variadic().is_some()
                })
                .map(|signature| {
                    dependencies.run(db, || {
                        signature.bind_self(db, self.checker.env, Some(self.implementation_self))
                    })
                })
                .collect::<Result<SmallVec<[_; 1]>, _>>()?;
            let signatures = CallableSignature { overloads };
            if signatures.overloads.is_empty() {
                // A negative alternative is absorbing for the existing `when_all` fold.
                return Ok(ProtocolMemberReadStep::Complete(self.checker.never()));
            }
            (
                dependencies.run(db, || callable.with_signatures(db, signatures))?,
                dependencies.run(db, || {
                    protocol_bind_self(
                        db,
                        self.checker.env.program(db),
                        self.required,
                        Some(self.protocol_self),
                    )
                })?,
            )
        } else {
            (callable, self.required)
        };
        Ok(ProtocolMemberReadStep::Callable(PendingReadCallable {
            source,
            target,
            alternatives: self,
        }))
    }
}

pub(super) struct PendingReadCallable<'checker, 'state, 'c, 'db> {
    pub(super) source: CallableType<'db>,
    pub(super) target: CallableType<'db>,
    alternatives: ClassMethodAlternatives<'checker, 'state, 'c, 'db>,
}

impl<'checker, 'state, 'c, 'db> PendingReadCallable<'checker, 'state, 'c, 'db> {
    pub(super) fn checker(&self) -> &'checker TypeRelationChecker<'state, 'c, 'db> {
        self.alternatives.checker
    }

    pub(super) fn resume<'member, D: MemberPairDependencies>(
        mut self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<ProtocolMemberReadStep<'checker, 'state, 'member, 'c, 'db>, D::Error> {
        match dependencies.run(db, || self.alternatives.fold.push(result))? {
            ControlFlow::Break(result) => Ok(ProtocolMemberReadStep::Complete(result)),
            ControlFlow::Continue(()) => self.alternatives.next(db, dependencies),
        }
    }
}
