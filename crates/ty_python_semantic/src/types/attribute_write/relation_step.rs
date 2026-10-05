//! Shared relation evaluation for resolved attribute writes.

use std::ops::ControlFlow;

use super::{
    AttributeWriteRequirement, ClassAttributeWriteMember, ExplicitAttributeWriteRequirement,
    FallbackAttributeWriteRequirement, InstanceAttributeWriteMember,
    ProtocolMemberWriteRequirement, attribute_write_requirement, descriptor_setter,
    instance_attribute_write_is_blocked, instance_setattr_dispatch, property_set_type,
};
use crate::place::{DefinedPlace, Place};
use crate::types::call::dunder::DunderCallRequest;
use crate::types::call::{Bindings, CallArguments, CallDunderError};
use crate::types::class::FrozenDataclassDispatch;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::dependencies::RelationDependencies;
use crate::types::{
    CallableSignature, CallableType, CallableTypes, MemberLookupPolicy, Signature, Type,
    TypeContext, TypeQualifiers, UpcastPolicy,
};
use crate::{Db, FxOrderSet};

#[cfg(test)]
mod tests;

/// Ordinary evaluation also serves finite representation controls. Each recursive callback here
/// must be replaced with its supervised child when these steps are driven by an attempt.
pub(super) fn evaluate<'db, 'c, D: RelationDependencies>(
    db: &'db dyn Db,
    input: WriteInput<'_, '_, '_, 'c, 'db>,
    operation: WriteOperation<'db>,
    dependencies: &D,
) -> Result<ConstraintSet<'db, 'c>, D::Error> {
    let mut step = AttributeWriteStep::start(db, input, operation, dependencies)?;
    let mut continuations: Vec<WriteContinuation<'_, '_, '_, 'c, 'db>> = Vec::new();
    loop {
        step = match step {
            AttributeWriteStep::Complete(result) => {
                let Some(continuation) = continuations.pop() else {
                    return Ok(result);
                };
                continuation.resume(db, result, dependencies)?
            }
            AttributeWriteStep::Evaluate(pending) => {
                continuations.push(pending.continuation);
                AttributeWriteStep::start(db, pending.input, pending.operation, dependencies)?
            }
            AttributeWriteStep::Resolve(pending) => {
                let requirement = dependencies.run(db, || {
                    attribute_write_requirement(
                        db,
                        pending.input.checker.env,
                        pending.object_ty,
                        pending.input.member_name,
                    )
                })?;
                pending.resume(db, requirement, dependencies)?
            }
            AttributeWriteStep::Relate(pending) => {
                AttributeWriteStep::Complete(dependencies.run(db, || {
                    pending.input.checker.check_type_pair(
                        db,
                        pending.input.value_ty,
                        pending.target,
                    )
                })?)
            }
            AttributeWriteStep::Convert(pending) => {
                let callables = dependencies.run(db, || {
                    pending.callable_ty.try_upcast_to_callable_with_policy(
                        db,
                        pending.input.checker.env,
                        UpcastPolicy::from(pending.input.checker.relation),
                    )
                })?;
                pending.resume(db, callables, dependencies)?
            }
            AttributeWriteStep::Lookup(pending) => {
                let place = dependencies.run(db, || match pending.lookup {
                    WriteLookup::DescriptorSetter(descriptor_ty) => {
                        descriptor_setter(db, pending.input.checker.env, descriptor_ty)
                    }
                    WriteLookup::SetAttr(object_ty) => {
                        object_ty
                            .member_lookup_with_policy(
                                db,
                                pending.input.checker.env,
                                "__setattr__",
                                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                                    | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                            )
                            .place
                    }
                })?;
                pending.resume(db, place, dependencies)?
            }
            AttributeWriteStep::Invoke(pending) => {
                let result = dependencies.run(db, || {
                    pending
                        .request
                        .evaluate(db, pending.input.checker.env, &pending.arguments)
                })?;
                pending.resume(db, &result, dependencies)?
            }
        };
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct WriteInput<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
    pub(in crate::types) member_name: &'name str,
    pub(in crate::types) value_ty: Type<'db>,
}

pub(in crate::types) enum WriteOperation<'db> {
    Resolve(Type<'db>),
    Requirement(AttributeWriteRequirement<'db>),
    Explicit {
        object_ty: Type<'db>,
        requirement: ExplicitAttributeWriteRequirement<'db>,
    },
    Fallback(FallbackAttributeWriteRequirement<'db>),
    Descriptor {
        descriptor_ty: Type<'db>,
        object_ty: Type<'db>,
    },
    CallableParameter {
        callable_ty: Type<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
    CallableSignatures {
        callable: CallableType<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
    SignatureParameter {
        signature: &'db Signature<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
}

pub(in crate::types) enum AttributeWriteStep<'checker, 'state, 'name, 'c, 'db> {
    Complete(ConstraintSet<'db, 'c>),
    Evaluate(PendingWriteOperation<'checker, 'state, 'name, 'c, 'db>),
    Resolve(PendingWriteRequirement<'checker, 'state, 'name, 'c, 'db>),
    Relate(PendingWriteRelation<'checker, 'state, 'name, 'c, 'db>),
    Convert(PendingWriteConversion<'checker, 'state, 'name, 'c, 'db>),
    Lookup(PendingWriteLookup<'checker, 'state, 'name, 'c, 'db>),
    Invoke(PendingWriteInvocation<'checker, 'state, 'name, 'c, 'db>),
}

impl<'checker, 'state, 'name, 'c, 'db> AttributeWriteStep<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn start<D: RelationDependencies>(
        db: &'db dyn Db,
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
        operation: WriteOperation<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let checker = input.checker;
        match operation {
            WriteOperation::Resolve(object_ty) => {
                Ok(Self::Resolve(PendingWriteRequirement { input, object_ty }))
            }
            WriteOperation::Requirement(requirement) => match requirement {
                AttributeWriteRequirement::All { element_tys, .. } => Ok(SequentialWrites {
                    elements: WriteElements::Slice(element_tys),
                    next: 0,
                    all: true,
                    result: checker.always(),
                }
                .next(input)),
                AttributeWriteRequirement::Any { intersection, .. } => Ok(SequentialWrites {
                    elements: WriteElements::Intersection(
                        dependencies.run(db, || intersection.positive(db))?,
                    ),
                    next: 0,
                    all: false,
                    result: checker.never(),
                }
                .next(input)),
                AttributeWriteRequirement::Unconstrained => Ok(Self::Complete(checker.always())),
                AttributeWriteRequirement::CannotAssign
                | AttributeWriteRequirement::Module(None)
                | AttributeWriteRequirement::ProtocolMember { write: None, .. } => {
                    Ok(Self::Complete(checker.never()))
                }
                AttributeWriteRequirement::Module(Some(write_ty))
                | AttributeWriteRequirement::ProtocolMember {
                    write: Some(ProtocolMemberWriteRequirement::AssignableTo(write_ty)),
                    ..
                } => Ok(Self::relate(input, write_ty)),
                AttributeWriteRequirement::ProtocolMember {
                    write: Some(ProtocolMemberWriteRequirement::Descriptor { domain, .. }),
                    ..
                } => Ok(Self::relate(input, domain.unwrap_or_else(Type::unknown))),
                AttributeWriteRequirement::Instance { object_ty, member } => {
                    let dispatch = dependencies.run(db, || {
                        instance_setattr_dispatch(db, checker.env, object_ty, input.member_name)
                    })?;
                    let receiver = dependencies.run(db, || {
                        dispatch.map_or(object_ty, |dispatch| {
                            dispatch.receiver(db, checker.env, object_ty)
                        })
                    })?;
                    let name_ty =
                        dependencies.run(db, || Type::string_literal(db, input.member_name))?;
                    let request = if matches!(dispatch, Some(FrozenDataclassDispatch::Delegate(_)))
                    {
                        // A generated frozen-dataclass setter calls bound super explicitly.
                        DunderCallRequest::on_class(receiver, "__setattr__", TypeContext::default())
                    } else {
                        DunderCallRequest::implicit(
                            receiver,
                            "__setattr__",
                            TypeContext::default(),
                            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                        )
                    };
                    Ok(Self::Invoke(PendingWriteInvocation {
                        input,
                        request,
                        arguments: CallArguments::positional([name_ty, input.value_ty]),
                        continuation: InvocationContinuation::SetAttr {
                            object_ty,
                            member,
                            dispatch,
                        },
                    }))
                }
                AttributeWriteRequirement::Class { object_ty, member } => match member {
                    ClassAttributeWriteMember::Explicit { member, fallback } => {
                        Ok(Self::explicit(input, object_ty, member, fallback, true))
                    }
                    ClassAttributeWriteMember::ClassAttribute(fallback) => {
                        Ok(Self::fallback(input, &fallback))
                    }
                    ClassAttributeWriteMember::Unresolved { .. } => {
                        Ok(Self::Complete(checker.never()))
                    }
                },
            },
            WriteOperation::Explicit {
                object_ty,
                requirement,
            } => {
                if requirement.qualifiers().contains(TypeQualifiers::FINAL) {
                    return Ok(Self::Complete(checker.never()));
                }
                match requirement {
                    ExplicitAttributeWriteRequirement::AssignableTo { ty, .. } => {
                        Ok(Self::relate(input, ty))
                    }
                    ExplicitAttributeWriteRequirement::Descriptor { descriptor_ty, .. } => {
                        if let Some(property) = descriptor_ty.as_property_instance()
                            && let Some(set_type) = dependencies.run(db, || {
                                property_set_type(db, checker.env, property, object_ty)
                            })?
                        {
                            return Ok(Self::relate(input, set_type));
                        }
                        Self::descriptor(db, input, descriptor_ty, object_ty, dependencies)
                    }
                }
            }
            WriteOperation::Fallback(requirement) => Ok(Self::fallback(input, &requirement)),
            WriteOperation::Descriptor {
                descriptor_ty,
                object_ty,
            } => Self::descriptor(db, input, descriptor_ty, object_ty, dependencies),
            WriteOperation::CallableParameter {
                callable_ty,
                parameter_index,
                self_ty,
            } => {
                if let Type::Union(union) = input.value_ty {
                    let elements = dependencies.run(db, || union.elements(db))?;
                    return FoldWrites::new(
                        checker,
                        FoldItems::Values {
                            elements,
                            callable_ty,
                            parameter_index,
                            self_ty,
                        },
                        ConstraintFoldKind::All,
                    )
                    .next(db, input, dependencies);
                }
                Ok(Self::Convert(PendingWriteConversion {
                    input,
                    callable_ty,
                    parameter_index,
                    self_ty,
                }))
            }
            WriteOperation::CallableSignatures {
                callable,
                parameter_index,
                self_ty,
            } => {
                let signatures = dependencies.run(db, || callable.signatures(db))?;
                FoldWrites::new(
                    checker,
                    FoldItems::Signatures {
                        signatures,
                        parameter_index,
                        self_ty,
                    },
                    ConstraintFoldKind::Any,
                )
                .next(db, input, dependencies)
            }
            WriteOperation::SignatureParameter {
                signature,
                parameter_index,
                self_ty,
            } => {
                let parameters = signature.parameters();
                let Some(parameter) = parameters.get_positional(parameter_index).or_else(|| {
                    parameters.variadic().and_then(|(index, parameter)| {
                        (index <= parameter_index).then_some(parameter)
                    })
                }) else {
                    return Ok(Self::Complete(checker.never()));
                };
                let write_ty = dependencies.run(db, || {
                    parameter
                        .annotated_type()
                        .bind_self_typevars(db, checker.env, self_ty)
                })?;
                Ok(Self::relate(input, write_ty))
            }
        }
    }

    fn relate(input: WriteInput<'checker, 'state, 'name, 'c, 'db>, target: Type<'db>) -> Self {
        Self::Relate(PendingWriteRelation { input, target })
    }

    fn fallback(
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
        requirement: &FallbackAttributeWriteRequirement<'db>,
    ) -> Self {
        match requirement {
            FallbackAttributeWriteRequirement::AssignableTo { qualifiers, .. }
                if qualifiers.contains(TypeQualifiers::FINAL) =>
            {
                Self::Complete(input.checker.never())
            }
            FallbackAttributeWriteRequirement::AssignableTo { ty, .. } => Self::relate(input, *ty),
            FallbackAttributeWriteRequirement::PossiblyMissing => {
                Self::Complete(input.checker.always())
            }
        }
    }

    fn explicit(
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
        object_ty: Type<'db>,
        requirement: ExplicitAttributeWriteRequirement<'db>,
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
        stop_if_never: bool,
    ) -> Self {
        Self::Evaluate(PendingWriteOperation {
            input,
            operation: WriteOperation::Explicit {
                object_ty,
                requirement,
            },
            continuation: WriteContinuation {
                input,
                action: WriteContinuationAction::AfterExplicit {
                    fallback,
                    stop_if_never,
                },
            },
        })
    }

    fn descriptor<D: RelationDependencies>(
        db: &'db dyn Db,
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
        descriptor_ty: Type<'db>,
        object_ty: Type<'db>,
        dependencies: &D,
    ) -> Result<Self, D::Error> {
        let descriptor_ty = dependencies.run(db, || descriptor_ty.resolve_type_alias(db))?;
        if let Type::Union(union) = descriptor_ty {
            let elements = dependencies.run(db, || union.elements(db))?;
            return FoldWrites::new(
                input.checker,
                FoldItems::Descriptors {
                    elements,
                    object_ty,
                },
                ConstraintFoldKind::All,
            )
            .next(db, input, dependencies);
        }
        Ok(Self::Invoke(PendingWriteInvocation {
            input,
            request: DunderCallRequest::implicit(
                descriptor_ty,
                "__set__",
                TypeContext::default(),
                MemberLookupPolicy::REQUIRE_CONCRETE,
            ),
            arguments: CallArguments::positional([object_ty, Type::unknown()]),
            continuation: InvocationContinuation::Descriptor { descriptor_ty },
        }))
    }
}

pub(in crate::types) struct PendingWriteRequirement<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) object_ty: Type<'db>,
}

impl<'checker, 'state, 'name, 'c, 'db> PendingWriteRequirement<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        requirement: AttributeWriteRequirement<'db>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        AttributeWriteStep::start(
            db,
            self.input,
            WriteOperation::Requirement(requirement),
            dependencies,
        )
    }
}

pub(in crate::types) struct PendingWriteRelation<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) target: Type<'db>,
}

pub(in crate::types) struct PendingWriteOperation<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) operation: WriteOperation<'db>,
    pub(in crate::types) continuation: WriteContinuation<'checker, 'state, 'name, 'c, 'db>,
}

pub(in crate::types) struct WriteContinuation<'checker, 'state, 'name, 'c, 'db> {
    input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    action: WriteContinuationAction<'db, 'c>,
}

enum WriteContinuationAction<'db, 'c> {
    Sequential(SequentialWrites<'db, 'c>),
    Fold(FoldWrites<'db, 'c>),
    AfterExplicit {
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
        stop_if_never: bool,
    },
    AfterFallback {
        explicit_result: ConstraintSet<'db, 'c>,
    },
}

impl<'checker, 'state, 'name, 'c, 'db> WriteContinuation<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: ConstraintSet<'db, 'c>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        let checker = self.input.checker;
        match self.action {
            WriteContinuationAction::Sequential(mut sequence) => {
                sequence.result = dependencies.run(db, || {
                    if sequence.all {
                        sequence.result.and(db, checker.constraints, || result)
                    } else {
                        sequence.result.or(db, checker.constraints, || result)
                    }
                })?;
                let terminal = if sequence.all {
                    sequence.result.is_trivially_never_satisfied()
                } else {
                    sequence.result.is_trivially_always_satisfied()
                };
                if terminal {
                    return Ok(AttributeWriteStep::Complete(sequence.result));
                }
                Ok(sequence.next(self.input))
            }
            WriteContinuationAction::Fold(mut fold) => {
                match dependencies.run(db, || fold.fold.push(result))? {
                    ControlFlow::Break(result) => Ok(AttributeWriteStep::Complete(result)),
                    ControlFlow::Continue(()) => fold.next(db, self.input, dependencies),
                }
            }
            WriteContinuationAction::AfterExplicit {
                fallback,
                stop_if_never,
            } => {
                // Class writes stop here; instance writes still evaluate their fallback.
                if stop_if_never && result.is_trivially_never_satisfied() {
                    return Ok(AttributeWriteStep::Complete(result));
                }
                let Some(fallback) = fallback else {
                    return Ok(AttributeWriteStep::Complete(result));
                };
                Ok(AttributeWriteStep::Evaluate(PendingWriteOperation {
                    input: self.input,
                    operation: WriteOperation::Fallback(fallback),
                    continuation: Self {
                        input: self.input,
                        action: WriteContinuationAction::AfterFallback {
                            explicit_result: result,
                        },
                    },
                }))
            }
            WriteContinuationAction::AfterFallback { explicit_result } => dependencies
                .run(db, || {
                    explicit_result.and(db, checker.constraints, || result)
                })
                .map(AttributeWriteStep::Complete),
        }
    }
}

enum WriteElements<'db> {
    Slice(&'db [Type<'db>]),
    Intersection(&'db FxOrderSet<Type<'db>>),
}

impl<'db> WriteElements<'db> {
    fn get(&self, index: usize) -> Option<Type<'db>> {
        match self {
            Self::Slice(elements) => elements.get(index).copied(),
            Self::Intersection(elements) => elements.get_index(index).copied(),
        }
    }
}

struct SequentialWrites<'db, 'c> {
    elements: WriteElements<'db>,
    next: usize,
    all: bool,
    result: ConstraintSet<'db, 'c>,
}

impl<'db, 'c> SequentialWrites<'db, 'c> {
    fn next<'checker, 'state, 'name>(
        mut self,
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    ) -> AttributeWriteStep<'checker, 'state, 'name, 'c, 'db> {
        let Some(object_ty) = self.elements.get(self.next) else {
            return AttributeWriteStep::Complete(self.result);
        };
        self.next += 1;
        AttributeWriteStep::Evaluate(PendingWriteOperation {
            input,
            operation: WriteOperation::Resolve(object_ty),
            continuation: WriteContinuation {
                input,
                action: WriteContinuationAction::Sequential(self),
            },
        })
    }
}

enum FoldItems<'db> {
    Descriptors {
        elements: &'db [Type<'db>],
        object_ty: Type<'db>,
    },
    Values {
        elements: &'db [Type<'db>],
        callable_ty: Type<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
    Callables {
        callables: CallableTypes<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
    Signatures {
        signatures: &'db CallableSignature<'db>,
        parameter_index: usize,
        self_ty: Type<'db>,
    },
}

struct FoldWrites<'db, 'c> {
    items: FoldItems<'db>,
    next: usize,
    fold: ConstraintFold<'db, 'c>,
}

impl<'db, 'c> FoldWrites<'db, 'c> {
    fn new(
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        items: FoldItems<'db>,
        kind: ConstraintFoldKind,
    ) -> Self {
        Self {
            items,
            next: 0,
            fold: ConstraintFold::new(checker.constraints, kind),
        }
    }

    fn next<'checker, 'state, 'name, D: RelationDependencies>(
        mut self,
        db: &'db dyn Db,
        input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        let mut child_input = input;
        let operation = match &self.items {
            FoldItems::Descriptors {
                elements,
                object_ty,
            } => elements
                .get(self.next)
                .map(|descriptor_ty| WriteOperation::Descriptor {
                    descriptor_ty: *descriptor_ty,
                    object_ty: *object_ty,
                }),
            FoldItems::Values {
                elements,
                callable_ty,
                parameter_index,
                self_ty,
            } => elements.get(self.next).map(|value_ty| {
                child_input.value_ty = *value_ty;
                WriteOperation::CallableParameter {
                    callable_ty: *callable_ty,
                    parameter_index: *parameter_index,
                    self_ty: *self_ty,
                }
            }),
            FoldItems::Callables {
                callables,
                parameter_index,
                self_ty,
            } => callables.iter().as_slice().get(self.next).map(|callable| {
                WriteOperation::CallableSignatures {
                    callable: *callable,
                    parameter_index: *parameter_index,
                    self_ty: *self_ty,
                }
            }),
            FoldItems::Signatures {
                signatures,
                parameter_index,
                self_ty,
            } => signatures.overloads.get(self.next).map(|signature| {
                WriteOperation::SignatureParameter {
                    signature,
                    parameter_index: *parameter_index,
                    self_ty: *self_ty,
                }
            }),
        };
        let Some(operation) = operation else {
            return dependencies
                .run(db, || self.fold.finish())
                .map(AttributeWriteStep::Complete);
        };
        self.next += 1;
        Ok(AttributeWriteStep::Evaluate(PendingWriteOperation {
            input: child_input,
            operation,
            continuation: WriteContinuation {
                input,
                action: WriteContinuationAction::Fold(self),
            },
        }))
    }
}

pub(in crate::types) struct PendingWriteConversion<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) callable_ty: Type<'db>,
    parameter_index: usize,
    self_ty: Type<'db>,
}

impl<'checker, 'state, 'name, 'c, 'db> PendingWriteConversion<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        callables: Option<CallableTypes<'db>>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        let Some(callables) = callables else {
            return Ok(AttributeWriteStep::Complete(self.input.checker.never()));
        };
        FoldWrites::new(
            self.input.checker,
            FoldItems::Callables {
                callables,
                parameter_index: self.parameter_index,
                self_ty: self.self_ty,
            },
            ConstraintFoldKind::All,
        )
        .next(db, self.input, dependencies)
    }
}

pub(in crate::types) enum WriteLookup<'db> {
    DescriptorSetter(Type<'db>),
    SetAttr(Type<'db>),
}

pub(in crate::types) struct PendingWriteLookup<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) lookup: WriteLookup<'db>,
}

impl<'checker, 'state, 'name, 'c, 'db> PendingWriteLookup<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        place: Place<'db>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        let Place::Defined(DefinedPlace {
            ty: callable_ty, ..
        }) = place
        else {
            return Ok(AttributeWriteStep::Complete(self.input.checker.never()));
        };
        let self_ty = match self.lookup {
            WriteLookup::DescriptorSetter(ty) | WriteLookup::SetAttr(ty) => ty,
        };
        AttributeWriteStep::start(
            db,
            self.input,
            WriteOperation::CallableParameter {
                callable_ty,
                parameter_index: 1,
                self_ty,
            },
            dependencies,
        )
    }
}

enum InvocationContinuation<'db> {
    SetAttr {
        object_ty: Type<'db>,
        member: InstanceAttributeWriteMember<'db>,
        dispatch: Option<FrozenDataclassDispatch<'db>>,
    },
    Descriptor {
        descriptor_ty: Type<'db>,
    },
}

pub(in crate::types) struct PendingWriteInvocation<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) input: WriteInput<'checker, 'state, 'name, 'c, 'db>,
    pub(in crate::types) request: DunderCallRequest<'static, 'db>,
    pub(in crate::types) arguments: CallArguments<'static, 'db>,
    continuation: InvocationContinuation<'db>,
}

impl<'checker, 'state, 'name, 'c, 'db> PendingWriteInvocation<'checker, 'state, 'name, 'c, 'db> {
    pub(in crate::types) fn resume<D: RelationDependencies>(
        self,
        db: &'db dyn Db,
        result: &Result<Bindings<'db>, CallDunderError<'db>>,
        dependencies: &D,
    ) -> Result<AttributeWriteStep<'checker, 'state, 'name, 'c, 'db>, D::Error> {
        match self.continuation {
            InvocationContinuation::SetAttr {
                object_ty,
                member,
                dispatch,
            } => {
                if dependencies.run(db, || {
                    instance_attribute_write_is_blocked(
                        db,
                        self.input.checker.env,
                        object_ty,
                        &member,
                        self.input.member_name,
                        result,
                        dispatch,
                    )
                })? {
                    return Ok(AttributeWriteStep::Complete(self.input.checker.never()));
                }
                match member {
                    InstanceAttributeWriteMember::ClassVar => {
                        Ok(AttributeWriteStep::Complete(self.input.checker.never()))
                    }
                    InstanceAttributeWriteMember::Explicit { member, fallback } => {
                        Ok(AttributeWriteStep::explicit(
                            self.input, object_ty, member, fallback, false,
                        ))
                    }
                    InstanceAttributeWriteMember::Instance(fallback) => {
                        Ok(AttributeWriteStep::fallback(self.input, &fallback))
                    }
                    InstanceAttributeWriteMember::SetAttr => {
                        if !matches!(result, Ok(_) | Err(CallDunderError::PossiblyUnbound { .. })) {
                            return Ok(AttributeWriteStep::Complete(self.input.checker.never()));
                        }
                        Ok(AttributeWriteStep::Lookup(PendingWriteLookup {
                            input: self.input,
                            lookup: WriteLookup::SetAttr(object_ty),
                        }))
                    }
                }
            }
            InvocationContinuation::Descriptor { descriptor_ty } => {
                if matches!(
                    result,
                    Err(CallDunderError::CallError(..) | CallDunderError::MethodNotAvailable)
                ) {
                    return Ok(AttributeWriteStep::Complete(self.input.checker.never()));
                }
                Ok(AttributeWriteStep::Lookup(PendingWriteLookup {
                    input: self.input,
                    lookup: WriteLookup::DescriptorSetter(descriptor_ty),
                }))
            }
        }
    }
}
