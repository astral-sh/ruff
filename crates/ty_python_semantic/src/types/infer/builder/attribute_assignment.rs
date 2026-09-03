use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use super::{ArgumentsIter, MultiInferenceGuard, TypeInferenceBuilder};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers};
use crate::types::attribute_write::{
    AttributeWriteRequirement, ClassAttributeWriteMember, ExplicitAttributeWriteRequirement,
    FallbackAttributeWriteRequirement, InstanceAttributeWriteMember,
    ProtocolMemberWriteRequirement, attribute_write_requirement, property_setter_returns_never,
};
use crate::types::call::{Bindings, CallArguments, CallDiagnosticOverride, CallError};
use crate::types::class::FrozenDataclassDispatch;
use crate::types::dedicated::pydantic;
use crate::types::diagnostic::{
    DEPRECATED, INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_ACCESS, MISSING_SLOT, UNRESOLVED_ATTRIBUTE,
    report_bad_dunder_set_call, report_invalid_attribute_assignment,
    report_possibly_missing_attribute,
};
use crate::types::{
    CallDunderError, DisplaySettings, MemberLookupPolicy, PropertyDeprecations, Type, TypeContext,
    TypeQualifiers,
};
use crate::{Db, ProgramEnvironment};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Make sure that the attribute assignment `obj.attribute = value` is valid.
    ///
    /// `target` is the node for the left-hand side, `object_ty` is the type of `obj`, `attribute` is
    /// the name of the attribute being assigned, `value` is the right-hand side, and `infer_value_ty`
    /// infers its type with the supplied context. If the assignment is invalid, emit diagnostics.
    pub(super) fn validate_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        value: &ast::Expr,
        object_ty: Type<'db>,
        attribute: &str,
        infer_value_ty: &mut dyn FnMut(&mut Self, TypeContext<'db>) -> Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.db();
        let requirement =
            attribute_write_requirement(db, self.program_environment(), object_ty, attribute);
        let mut deprecation = (emit_diagnostics && self.context.is_lint_enabled(&DEPRECATED))
            .then_some(AttributeDeprecation::Missing);
        let mut evaluator = AssignmentAttributeWriteEvaluator {
            builder: self,
            target,
            value,
            object_ty,
            attribute,
            infer_value_ty: MultiInferenceGuard::new(infer_value_ty),
        };
        let valid = evaluator.evaluate(&requirement, emit_diagnostics, deprecation.as_mut());
        if let Some(AttributeDeprecation::Deprecated(properties)) = deprecation {
            self.check_deprecated_property(target, properties, ast::ExprContext::Store);
        }
        valid
    }
}

enum AssignmentAttributeWriteDiagnostic<'db> {
    InvalidCompositeAssignment {
        object_ty: Type<'db>,
        value_ty: Type<'db>,
    },
    CannotAssign,
    CannotAssignToClassVar,
    TerminalSetAttr {
        member_exists: bool,
        is_setattr_synthesized: bool,
    },
    TerminalDescriptor,
    BadDunderSet {
        failure: CallError<'db>,
        descriptor_ty: Type<'db>,
        includes_descriptor_argument: bool,
    },
    PossiblyMissing,
    BadSetAttr {
        value_ty: Type<'db>,
        failure: CallError<'db>,
    },
    Unresolved {
        with_period: bool,
    },
    CannotAssignToInstanceAttribute,
}

#[derive(Clone, Copy)]
enum ContextualInference {
    Commit,
    Speculate,
}

/// Whether a resolved write target contributes or suppresses a property deprecation.
#[derive(Clone, Copy)]
enum AttributeDeprecation<'db> {
    /// No declared member provides an alternative to another member's deprecated accessor.
    Missing,
    /// A non-deprecated member can provide the implementation in an intersection.
    NotDeprecated,
    /// Accessor deprecations checked at the assignment site.
    Deprecated(PropertyDeprecations<'db>),
}

impl<'db> AttributeDeprecation<'db> {
    /// Inspect a resolved requirement without inferring or validating the assigned value.
    /// Only union and intersection children need further attribute lookups.
    fn from_requirement(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        requirement: &AttributeWriteRequirement<'db>,
        attribute: &str,
    ) -> Self {
        match requirement {
            AttributeWriteRequirement::All { element_tys, .. } => {
                element_tys.iter().fold(Self::Missing, |deprecation, ty| {
                    let requirement = attribute_write_requirement(db, env, *ty, attribute);
                    deprecation.union(db, Self::from_requirement(db, env, &requirement, attribute))
                })
            }
            AttributeWriteRequirement::Any { intersection, .. } => {
                let mut deprecation = Self::Missing;
                for ty in intersection.positive(db) {
                    let requirement = attribute_write_requirement(db, env, *ty, attribute);
                    deprecation = deprecation
                        .intersection(db, Self::from_requirement(db, env, &requirement, attribute));
                    if matches!(deprecation, Self::NotDeprecated) {
                        break;
                    }
                }
                deprecation
            }
            AttributeWriteRequirement::ProtocolMember {
                write: Some(ProtocolMemberWriteRequirement::Descriptor { descriptor_ty, .. }),
                ..
            }
            | AttributeWriteRequirement::Instance {
                member:
                    InstanceAttributeWriteMember::Explicit {
                        member: ExplicitAttributeWriteRequirement::Descriptor { descriptor_ty, .. },
                        ..
                    },
                ..
            }
            | AttributeWriteRequirement::Class {
                member:
                    ClassAttributeWriteMember::Explicit {
                        member: ExplicitAttributeWriteRequirement::Descriptor { descriptor_ty, .. },
                        ..
                    },
                ..
            } if let Some(properties) = descriptor_ty.property_deprecations(db) => {
                Self::Deprecated(properties)
            }
            AttributeWriteRequirement::Instance {
                member: InstanceAttributeWriteMember::SetAttr,
                ..
            }
            | AttributeWriteRequirement::Class {
                member: ClassAttributeWriteMember::Unresolved { .. },
                ..
            }
            | AttributeWriteRequirement::Module(None) => Self::Missing,
            _ => Self::NotDeprecated,
        }
    }

    /// A union can invoke either target, so either target can contribute a deprecation.
    fn union(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Self::Deprecated(left), Self::Deprecated(right)) => {
                Self::Deprecated(left.union(db, right))
            }
            (deprecated @ Self::Deprecated(_), _) | (_, deprecated @ Self::Deprecated(_)) => {
                deprecated
            }
            (Self::NotDeprecated, _) | (_, Self::NotDeprecated) => Self::NotDeprecated,
            (Self::Missing, Self::Missing) => Self::Missing,
        }
    }

    /// An intersection can use a non-deprecated member instead, but an absent member cannot
    /// provide an alternative implementation.
    fn intersection(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Self::NotDeprecated, _) | (_, Self::NotDeprecated) => Self::NotDeprecated,
            (Self::Deprecated(left), Self::Deprecated(right)) => {
                Self::Deprecated(left.intersection(db, right))
            }
            (Self::Missing, other) | (other, Self::Missing) => other,
        }
    }
}

struct AssignmentAttributeWriteEvaluator<'a, 'db, 'ast, 'infer> {
    builder: &'a mut TypeInferenceBuilder<'db, 'ast>,
    target: &'a ast::ExprAttribute,
    value: &'a ast::Expr,
    object_ty: Type<'db>,
    attribute: &'a str,
    infer_value_ty: MultiInferenceGuard<'db, 'ast, 'infer>,
}

impl<'db> AssignmentAttributeWriteEvaluator<'_, 'db, '_, '_> {
    fn infer_value(&mut self, tcx: TypeContext<'db>, emit_diagnostics: bool) -> Type<'db> {
        if emit_diagnostics {
            self.infer_value_ty.infer_loud(self.builder, tcx)
        } else {
            self.infer_value_ty.infer_silent(self.builder, tcx)
        }
    }

    /// Infer the value again using the context that succeeded.
    ///
    /// The earlier inference was only a trial, so its result was not saved.
    fn infer_with_last_context(&mut self, emit_diagnostics: bool) -> Type<'db> {
        self.infer_value(self.infer_value_ty.last_tcx(), emit_diagnostics)
    }

    /// Infer an attribute-assignment value using the context provided by `__setattr__`, then
    /// validate the synthesized setter call and return both its result and the inferred value type.
    ///
    /// ```python
    /// from collections.abc import Callable
    ///
    /// class Custom:
    ///     def __setattr__(self, name: str, value: Callable[[int], int]) -> None: ...
    ///
    /// instance = Custom()
    /// instance.callback = lambda value: value + 1  # `value` is inferred as `int`.
    /// ```
    fn infer_and_try_call_setattr(
        &mut self,
        object_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> (Result<Bindings<'db>, CallDunderError<'db>>, Type<'db>) {
        let db = self.builder.db();
        let name_ty = Type::string_literal(db, self.attribute);
        let ast_arguments = [
            ast::ArgOrKeyword::Arg(self.target.value.as_ref()),
            ast::ArgOrKeyword::Arg(self.value),
        ];
        let mut call_arguments = CallArguments::positional([name_ty, Type::unknown()]);
        // A bound `super` must use its own MRO lookup rather than the normal instance fallback.
        let lookup_policy = if matches!(object_ty, Type::BoundSuper(_)) {
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
        } else {
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK | MemberLookupPolicy::NO_INSTANCE_FALLBACK
        };
        let setattr_result = self.builder.infer_and_try_call_dunder(
            object_ty,
            "__setattr__",
            lookup_policy,
            ArgumentsIter::synthesized(&ast_arguments),
            &mut call_arguments,
            &mut |builder, (argument_index, _, tcx)| {
                if argument_index == 0 {
                    name_ty
                } else {
                    self.infer_value_ty.infer_silent(builder, tcx)
                }
            },
            TypeContext::default(),
        );
        let value_ty = self.infer_with_last_context(emit_diagnostics);
        (setattr_result, value_ty)
    }

    /// Validate the assignment and optionally record its accessor deprecations.
    /// After validation short-circuits, a pure collector inspects the remaining alternatives
    /// without changing the inference context selected for the assigned value.
    /// When provided, `deprecation` receives the result even if validation fails.
    fn evaluate(
        &mut self,
        requirement: &AttributeWriteRequirement<'db>,
        emit_diagnostics: bool,
        mut deprecation: Option<&mut AttributeDeprecation<'db>>,
    ) -> bool {
        let db = self.builder.db();
        let env = self.builder.program_environment();
        if let Some(deprecation) = deprecation.as_deref_mut() {
            *deprecation = match requirement {
                AttributeWriteRequirement::All { .. } | AttributeWriteRequirement::Any { .. } => {
                    AttributeDeprecation::Missing
                }
                _ => AttributeDeprecation::from_requirement(db, env, requirement, self.attribute),
            };
        }

        match requirement {
            AttributeWriteRequirement::All {
                object_ty,
                element_tys,
            } => {
                let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);
                let attribute = self.attribute;
                let mut requirements = element_tys
                    .iter()
                    .map(|ty| attribute_write_requirement(db, env, *ty, attribute));
                let valid = requirements.by_ref().all(|requirement| {
                    let mut current = AttributeDeprecation::Missing;
                    let valid = self.evaluate(
                        &requirement,
                        false,
                        deprecation.as_ref().map(|_| &mut current),
                    );
                    if let Some(deprecation) = deprecation.as_deref_mut() {
                        *deprecation = deprecation.union(db, current);
                    }
                    valid
                });
                if let Some(deprecation) = deprecation {
                    *deprecation = requirements.fold(*deprecation, |deprecation, requirement| {
                        deprecation.union(
                            db,
                            AttributeDeprecation::from_requirement(
                                db,
                                env,
                                &requirement,
                                attribute,
                            ),
                        )
                    });
                }
                if valid {
                    self.validate_composite_final_assignment(*object_ty, emit_diagnostics);
                } else if emit_diagnostics {
                    self.report(
                        AssignmentAttributeWriteDiagnostic::InvalidCompositeAssignment {
                            object_ty: *object_ty,
                            value_ty,
                        },
                    );
                }
                valid
            }
            AttributeWriteRequirement::Any {
                object_ty,
                intersection,
            } => {
                let attribute = self.attribute;
                let mut requirements = intersection
                    .positive(db)
                    .iter()
                    .map(|ty| attribute_write_requirement(db, env, *ty, attribute));
                let valid = requirements.by_ref().any(|requirement| {
                    let mut current = AttributeDeprecation::Missing;
                    let valid = self.evaluate(
                        &requirement,
                        false,
                        deprecation.as_ref().map(|_| &mut current),
                    );
                    if let Some(deprecation) = deprecation.as_deref_mut() {
                        *deprecation = deprecation.intersection(db, current);
                    }
                    valid
                });
                if let Some(deprecation) = deprecation {
                    while !matches!(deprecation, AttributeDeprecation::NotDeprecated)
                        && let Some(requirement) = requirements.next()
                    {
                        *deprecation = deprecation.intersection(
                            db,
                            AttributeDeprecation::from_requirement(
                                db,
                                env,
                                &requirement,
                                attribute,
                            ),
                        );
                    }
                }
                if valid {
                    self.infer_with_last_context(emit_diagnostics);
                    self.validate_composite_final_assignment(*object_ty, emit_diagnostics);
                } else {
                    let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);
                    if emit_diagnostics {
                        self.report(
                            AssignmentAttributeWriteDiagnostic::InvalidCompositeAssignment {
                                object_ty: *object_ty,
                                value_ty,
                            },
                        );
                    }
                }
                valid
            }
            AttributeWriteRequirement::Unconstrained => {
                self.infer_value(TypeContext::default(), emit_diagnostics);
                true
            }
            AttributeWriteRequirement::CannotAssign => {
                self.infer_value(TypeContext::default(), emit_diagnostics);
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::CannotAssign);
                }
                false
            }
            AttributeWriteRequirement::Module(write_ty) => {
                if let Some(write_ty) = write_ty {
                    let value_ty =
                        self.infer_value(TypeContext::new(Some(*write_ty)), emit_diagnostics);
                    self.check_type_pair(value_ty, *write_ty, emit_diagnostics)
                } else {
                    self.infer_value(TypeContext::default(), emit_diagnostics);
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::Unresolved {
                            with_period: true,
                        });
                    }
                    false
                }
            }
            AttributeWriteRequirement::ProtocolMember { write, qualifiers } => match write {
                Some(ProtocolMemberWriteRequirement::AssignableTo(write_ty)) => {
                    let value_ty =
                        self.infer_value(TypeContext::new(Some(*write_ty)), emit_diagnostics);
                    self.check_type_pair(value_ty, *write_ty, emit_diagnostics)
                }
                Some(ProtocolMemberWriteRequirement::Descriptor {
                    descriptor_ty,
                    receiver_ty,
                    domain,
                }) => {
                    let value_ty = self.infer_value(
                        TypeContext::new(Some(domain.unwrap_or_else(Type::unknown))),
                        emit_diagnostics,
                    );
                    if let Some(domain) = domain
                        && !self.check_type_pair(value_ty, *domain, emit_diagnostics)
                    {
                        return false;
                    }
                    self.evaluate_protocol_descriptor_write(
                        *descriptor_ty,
                        *receiver_ty,
                        value_ty,
                        emit_diagnostics,
                    )
                }
                None => {
                    self.infer_value(TypeContext::default(), emit_diagnostics);
                    let reported_final = !qualifiers.contains(TypeQualifiers::CLASS_VAR)
                        && qualifiers.contains(TypeQualifiers::FINAL)
                        && !self.final_assignment_is_valid(
                            self.object_ty,
                            *qualifiers,
                            emit_diagnostics,
                        );
                    if emit_diagnostics && !reported_final {
                        self.report(if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                            AssignmentAttributeWriteDiagnostic::CannotAssignToClassVar
                        } else {
                            AssignmentAttributeWriteDiagnostic::CannotAssign
                        });
                    }
                    false
                }
            },
            AttributeWriteRequirement::Instance { object_ty, member } => {
                self.evaluate_instance(*object_ty, member, emit_diagnostics)
            }
            AttributeWriteRequirement::Class { object_ty, member } => {
                self.evaluate_class(*object_ty, member, emit_diagnostics)
            }
        }
    }

    fn check_type_pair(
        &mut self,
        value_ty: Type<'db>,
        target_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.builder.db();
        let assignable =
            value_ty.is_assignable_to(db, self.builder.program_environment(), target_ty);
        if !assignable && emit_diagnostics {
            report_invalid_attribute_assignment(
                &self.builder.context,
                self.target.range(),
                target_ty,
                value_ty,
                self.attribute,
            );
        }
        assignable
    }

    fn final_assignment_is_valid(
        &mut self,
        object_ty: Type<'db>,
        qualifiers: TypeQualifiers,
        emit_diagnostics: bool,
    ) -> bool {
        !(emit_diagnostics
            && self.builder.invalid_assignment_to_final_attribute(
                object_ty,
                self.target,
                self.attribute,
                qualifiers,
            ))
    }

    fn validate_composite_final_assignment(
        &mut self,
        object_ty: Type<'db>,
        emit_diagnostics: bool,
    ) {
        if emit_diagnostics {
            self.builder.validate_final_attribute_assignment(
                self.target,
                object_ty,
                self.attribute,
            );
        }
    }

    fn evaluate_instance(
        &mut self,
        object_ty: Type<'db>,
        member: &InstanceAttributeWriteMember<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.builder.db();
        let env = self.builder.program_environment();

        let frozen_dataclass_dispatch = object_ty
            .nominal_class(db, env)
            .and_then(|class| class.static_class_literal(db))
            .and_then(|(class, specialization)| {
                class.inherited_frozen_dataclass_dispatch(
                    db,
                    specialization,
                    "__setattr__",
                    self.attribute,
                )
            });
        let setattr_receiver = frozen_dataclass_dispatch
            .map_or(object_ty, |dispatch| dispatch.receiver(db, env, object_ty));

        let (setattr_result, value_ty) = if matches!(member, InstanceAttributeWriteMember::SetAttr)
            || matches!(
                frozen_dataclass_dispatch,
                Some(FrozenDataclassDispatch::Delegate(_))
            ) {
            self.infer_and_try_call_setattr(setattr_receiver, emit_diagnostics)
        } else {
            let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);
            let setattr_result = setattr_receiver.try_call_dunder_with_policy(
                db,
                env,
                "__setattr__",
                &mut CallArguments::positional([
                    Type::string_literal(db, self.attribute),
                    value_ty,
                ]),
                TypeContext::default(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            );
            (setattr_result, value_ty)
        };

        // A terminal `__setattr__` blocks even explicitly declared attributes.
        let setattr_returns_never = matches!(
            frozen_dataclass_dispatch,
            Some(FrozenDataclassDispatch::FrozenField)
        ) || match &setattr_result {
            Ok(bindings) => bindings.return_type(db, env).is_never(),
            Err(error) => error.return_type(db, env).is_some_and(|ty| ty.is_never()),
        };

        // We could also model this more precisely by synthesizing a `__setattr__`overload set
        // that only disallows mutation on non-private fields, but for now, we just suppress the
        // diagnostic here. This is much easier and faster.
        let is_private_pydantic_attribute =
            matches!(member, InstanceAttributeWriteMember::Explicit { .. })
                && pydantic::is_private_attribute(self.attribute)
                && pydantic::is_model_instance(db, env, object_ty);

        if setattr_returns_never && !is_private_pydantic_attribute {
            if emit_diagnostics {
                let is_setattr_synthesized = !matches!(
                    frozen_dataclass_dispatch,
                    Some(FrozenDataclassDispatch::Delegate(_))
                ) && match object_ty.class_member_with_policy(
                    db,
                    env,
                    "__setattr__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                ) {
                    PlaceAndQualifiers {
                        place: Place::Defined(DefinedPlace { ty, .. }),
                        ..
                    } => ty.is_callable_type(),
                    _ => false,
                };
                let member_exists = !object_ty
                    .member(db, env, self.attribute)
                    .place
                    .is_undefined();
                self.report(AssignmentAttributeWriteDiagnostic::TerminalSetAttr {
                    member_exists,
                    is_setattr_synthesized,
                });
            }
            return false;
        }

        match member {
            InstanceAttributeWriteMember::ClassVar => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::CannotAssignToClassVar);
                }
                false
            }
            InstanceAttributeWriteMember::Explicit { member, fallback } => {
                if !self.final_assignment_is_valid(object_ty, member.qualifiers(), emit_diagnostics)
                {
                    return false;
                }
                if matches!(
                    frozen_dataclass_dispatch,
                    Some(FrozenDataclassDispatch::Delegate(_))
                ) && let Err(CallDunderError::CallError(kind, bindings, _)) = setattr_result
                {
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::BadSetAttr {
                            value_ty,
                            failure: CallError(kind, bindings),
                        });
                    }
                    return false;
                }
                let member_valid =
                    self.evaluate_explicit_member(object_ty, member, value_ty, emit_diagnostics);
                if let Some(fallback) = fallback {
                    let fallback_valid =
                        self.evaluate_instance_fallback(object_ty, fallback, emit_diagnostics);
                    member_valid && fallback_valid
                } else {
                    member_valid
                }
            }
            InstanceAttributeWriteMember::Instance(fallback) => {
                self.evaluate_instance_fallback(object_ty, fallback, emit_diagnostics)
            }
            InstanceAttributeWriteMember::SetAttr => match setattr_result {
                Ok(_) | Err(CallDunderError::PossiblyUnbound { .. }) => true,
                Err(CallDunderError::CallError(kind, bindings, _)) => {
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::BadSetAttr {
                            value_ty,
                            failure: CallError(kind, bindings),
                        });
                    }
                    false
                }
                Err(CallDunderError::MethodNotAvailable)
                    if matches!(
                        frozen_dataclass_dispatch,
                        Some(FrozenDataclassDispatch::Delegate(_))
                    ) =>
                {
                    true
                }
                Err(CallDunderError::MethodNotAvailable) => {
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::Unresolved {
                            with_period: false,
                        });
                    }
                    false
                }
            },
        }
    }

    fn evaluate_class(
        &mut self,
        object_ty: Type<'db>,
        member: &ClassAttributeWriteMember<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.builder.db();
        let env = self.builder.program_environment();
        match member {
            ClassAttributeWriteMember::Explicit { member, fallback } => {
                if !self.final_assignment_is_valid(object_ty, member.qualifiers(), emit_diagnostics)
                {
                    self.infer_value(TypeContext::default(), emit_diagnostics);
                    return false;
                }
                let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);
                let member_valid =
                    self.evaluate_explicit_member(object_ty, member, value_ty, emit_diagnostics);
                if let Some(fallback) = fallback {
                    let fallback_valid = self.evaluate_class_fallback(
                        object_ty,
                        fallback,
                        emit_diagnostics,
                        ContextualInference::Speculate,
                    );
                    member_valid && fallback_valid
                } else {
                    member_valid
                }
            }
            ClassAttributeWriteMember::ClassAttribute(fallback) => self.evaluate_class_fallback(
                object_ty,
                fallback,
                emit_diagnostics,
                ContextualInference::Commit,
            ),
            ClassAttributeWriteMember::Unresolved {
                has_instance_attribute,
            } => {
                let (setattr_result, value_ty) =
                    self.infer_and_try_call_setattr(object_ty, emit_diagnostics);
                let setattr_returns_never = match &setattr_result {
                    Ok(bindings) => bindings.return_type(db, env).is_never(),
                    Err(error) => error.return_type(db, env).is_some_and(|ty| ty.is_never()),
                };
                if setattr_returns_never {
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::TerminalSetAttr {
                            member_exists: false,
                            is_setattr_synthesized: false,
                        });
                    }
                    return false;
                }

                match setattr_result {
                    Ok(_) | Err(CallDunderError::PossiblyUnbound { .. }) => true,
                    Err(CallDunderError::CallError(kind, bindings, _)) => {
                        if emit_diagnostics {
                            self.report(AssignmentAttributeWriteDiagnostic::BadSetAttr {
                                value_ty,
                                failure: CallError(kind, bindings),
                            });
                        }
                        false
                    }
                    Err(CallDunderError::MethodNotAvailable) => {
                        if emit_diagnostics {
                            self.report(if *has_instance_attribute {
                                AssignmentAttributeWriteDiagnostic::CannotAssignToInstanceAttribute
                            } else {
                                AssignmentAttributeWriteDiagnostic::Unresolved { with_period: true }
                            });
                        }
                        false
                    }
                }
            }
        }
    }

    fn evaluate_explicit_member(
        &mut self,
        object_ty: Type<'db>,
        requirement: &ExplicitAttributeWriteRequirement<'db>,
        value_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        match requirement {
            ExplicitAttributeWriteRequirement::Descriptor {
                descriptor_ty,
                setter_ty,
                ..
            } => self.evaluate_descriptor_write(
                *descriptor_ty,
                *setter_ty,
                object_ty,
                value_ty,
                emit_diagnostics,
            ),
            ExplicitAttributeWriteRequirement::AssignableTo { ty, .. } => {
                let value_ty = self.infer_value(TypeContext::new(Some(*ty)), false);
                self.check_type_pair(value_ty, *ty, emit_diagnostics)
            }
        }
    }

    fn evaluate_protocol_descriptor_write(
        &mut self,
        descriptor_ty: Type<'db>,
        receiver_ty: Type<'db>,
        value_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let env = self.builder.program_environment();
        let db = self.builder.db();
        let descriptor_ty = descriptor_ty.resolve_type_alias(db);
        if let Type::Union(union) = descriptor_ty {
            for descriptor_ty in union.elements(db) {
                if !self.evaluate_protocol_descriptor_write(
                    *descriptor_ty,
                    receiver_ty,
                    value_ty,
                    false,
                ) {
                    if emit_diagnostics {
                        self.evaluate_protocol_descriptor_write(
                            *descriptor_ty,
                            receiver_ty,
                            value_ty,
                            true,
                        );
                    }
                    return false;
                }
            }
            return true;
        }

        if property_setter_returns_never(db, env, descriptor_ty, receiver_ty, value_ty) {
            if emit_diagnostics {
                self.report(AssignmentAttributeWriteDiagnostic::TerminalDescriptor);
            }
            return false;
        }

        match descriptor_ty.try_call_dunder_with_policy(
            db,
            env,
            "__set__",
            &mut CallArguments::positional([receiver_ty, value_ty]),
            TypeContext::default(),
            MemberLookupPolicy::REQUIRE_CONCRETE,
        ) {
            Ok(_) => true,
            Err(CallDunderError::CallError(kind, bindings, _)) => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::BadDunderSet {
                        failure: CallError(kind, bindings),
                        descriptor_ty,
                        includes_descriptor_argument: false,
                    });
                }
                false
            }
            Err(CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound { .. }) => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::CannotAssign);
                }
                false
            }
        }
    }

    fn evaluate_descriptor_write(
        &mut self,
        descriptor_ty: Type<'db>,
        setter_ty: Type<'db>,
        object_ty: Type<'db>,
        value_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.builder.db();
        let env = self.builder.program_environment();
        let setter_result = setter_ty.try_call(
            db,
            env,
            &CallArguments::positional([descriptor_ty, object_ty, value_ty]),
        );
        // `Never` supports arbitrary operations only because there can be no runtime value to
        // mutate; it is not a concrete descriptor with a terminal setter.
        let setter_returns_never = !descriptor_ty.is_never()
            && match &setter_result {
                Ok(bindings) => bindings.return_type(db, env).is_never(),
                Err(error) => error.return_type(db, env).is_never(),
            };
        if setter_returns_never
            || property_setter_returns_never(db, env, descriptor_ty, object_ty, value_ty)
        {
            if emit_diagnostics {
                self.report(AssignmentAttributeWriteDiagnostic::TerminalDescriptor);
            }
            return false;
        }

        match setter_result {
            Ok(_) => true,
            Err(error) => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::BadDunderSet {
                        failure: error,
                        descriptor_ty,
                        includes_descriptor_argument: true,
                    });
                }
                false
            }
        }
    }

    fn evaluate_instance_fallback(
        &mut self,
        object_ty: Type<'db>,
        requirement: &FallbackAttributeWriteRequirement<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        match requirement {
            FallbackAttributeWriteRequirement::AssignableTo {
                ty,
                qualifiers,
                possibly_missing,
            } => {
                if !self.final_assignment_is_valid(object_ty, *qualifiers, emit_diagnostics) {
                    return false;
                }
                let value_ty = self.infer_value(TypeContext::new(Some(*ty)), false);
                let valid = self.check_type_pair(value_ty, *ty, emit_diagnostics);
                if *possibly_missing {
                    self.report(AssignmentAttributeWriteDiagnostic::PossiblyMissing);
                }
                valid
            }
            FallbackAttributeWriteRequirement::PossiblyMissing => {
                self.report(AssignmentAttributeWriteDiagnostic::PossiblyMissing);
                true
            }
        }
    }

    fn evaluate_class_fallback(
        &mut self,
        object_ty: Type<'db>,
        requirement: &FallbackAttributeWriteRequirement<'db>,
        emit_diagnostics: bool,
        inference: ContextualInference,
    ) -> bool {
        match requirement {
            FallbackAttributeWriteRequirement::AssignableTo {
                ty,
                qualifiers,
                possibly_missing,
            } => {
                let value_ty = self.infer_value(
                    TypeContext::new(Some(*ty)),
                    matches!(inference, ContextualInference::Commit) && emit_diagnostics,
                );
                if !self.builder.validate_generic_class_attribute_access(
                    self.target,
                    object_ty,
                    emit_diagnostics,
                ) {
                    return false;
                }
                if !self.final_assignment_is_valid(object_ty, *qualifiers, emit_diagnostics) {
                    return false;
                }
                let valid = self.check_type_pair(value_ty, *ty, emit_diagnostics);
                if *possibly_missing {
                    self.report(AssignmentAttributeWriteDiagnostic::PossiblyMissing);
                }
                valid
            }
            FallbackAttributeWriteRequirement::PossiblyMissing => {
                self.report(AssignmentAttributeWriteDiagnostic::PossiblyMissing);
                true
            }
        }
    }

    fn report(&mut self, diagnostic: AssignmentAttributeWriteDiagnostic<'db>) {
        let db = self.builder.db();
        let env = self.builder.program_environment();
        match diagnostic {
            AssignmentAttributeWriteDiagnostic::InvalidCompositeAssignment {
                object_ty,
                value_ty,
            } => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, self.target)
                {
                    let settings = DisplaySettings::from_possibly_ambiguous_types(
                        db,
                        env,
                        [value_ty, object_ty],
                    );
                    builder.into_diagnostic(format_args!(
                        "Object of type `{}` is not assignable to attribute `{}` on type `{}`",
                        value_ty.display_with(db, env, settings.clone()),
                        self.attribute,
                        object_ty.display_with(db, env, settings),
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::CannotAssign => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, self.target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to attribute `{}` on type `{}`",
                        self.attribute,
                        self.object_ty.display(db, env),
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::CannotAssignToClassVar => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ATTRIBUTE_ACCESS, self.target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to ClassVar `{}` from an instance of type `{}`",
                        self.attribute,
                        self.object_ty.display(db, env),
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::TerminalSetAttr {
                member_exists,
                is_setattr_synthesized,
            } => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, self.target)
                {
                    let message = if !member_exists {
                        format!(
                            "Cannot assign to unresolved attribute `{}` on type `{}`",
                            self.attribute,
                            self.object_ty.display(db, env)
                        )
                    } else if is_setattr_synthesized {
                        format!(
                            "Property `{}` defined in `{}` is read-only",
                            self.attribute,
                            self.object_ty.display(db, env)
                        )
                    } else {
                        format!(
                            "Cannot assign to attribute `{}` on type `{}` whose `__setattr__` method returns `Never`/`NoReturn`",
                            self.attribute,
                            self.object_ty.display(db, env)
                        )
                    };
                    builder.into_diagnostic(message);
                }
            }
            AssignmentAttributeWriteDiagnostic::TerminalDescriptor => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, self.target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to attribute `{}` on type `{}` whose `__set__` method returns `Never`/`NoReturn`",
                        self.attribute,
                        self.object_ty.display(db, env),
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::BadDunderSet {
                failure,
                descriptor_ty,
                includes_descriptor_argument,
            } => {
                report_bad_dunder_set_call(
                    &self.builder.context,
                    &failure,
                    self.object_ty,
                    descriptor_ty,
                    includes_descriptor_argument,
                    self.target,
                    self.value,
                );
            }
            AssignmentAttributeWriteDiagnostic::PossiblyMissing => {
                report_possibly_missing_attribute(
                    &self.builder.context,
                    self.target,
                    self.attribute,
                    self.object_ty,
                );
            }
            AssignmentAttributeWriteDiagnostic::BadSetAttr { value_ty, failure } => {
                failure.report_diagnostics_with_override(
                    &self.builder.context,
                    self.target.into(),
                    &CallDiagnosticOverride {
                        lint: &INVALID_ASSIGNMENT,
                        message: format!(
                            "Cannot assign object of type `{}` to attribute `{}` on type `{}`",
                            value_ty.display(db, env),
                            self.attribute,
                            self.object_ty.display(db, env)
                        ),
                        info: "This assignment implicitly calls a custom `__setattr__` method",
                        argument_ranges: &[self.target.range(), self.value.range()],
                    },
                );
            }
            AssignmentAttributeWriteDiagnostic::Unresolved { with_period: false }
                if self
                    .object_ty
                    .nominal_class(db, env)
                    .and_then(|class| class.static_class_literal(db))
                    .is_some_and(|(class, _)| class.lacks_instance_storage(db, self.attribute))
                    && !self
                        .object_ty
                        .class_member(db, env, self.attribute)
                        .place
                        .is_undefined() =>
            {
                if let Some(builder) = self.builder.context.report_lint(&MISSING_SLOT, self.target)
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot assign to attribute `{}`: `{}` has no slot or instance dictionary",
                        self.attribute,
                        self.object_ty.display(db, env),
                    ));
                    diagnostic.info(format_args!(
                        "Attribute `{}` is declared but is not included in `__slots__`",
                        self.attribute,
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::Unresolved { with_period } => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&UNRESOLVED_ATTRIBUTE, self.target)
                {
                    if with_period {
                        builder.into_diagnostic(format_args!(
                            "Unresolved attribute `{}` on type `{}`.",
                            self.attribute,
                            self.object_ty.display(db, env)
                        ));
                    } else {
                        builder.into_diagnostic(format_args!(
                            "Unresolved attribute `{}` on type `{}`",
                            self.attribute,
                            self.object_ty.display(db, env)
                        ));
                    }
                }
            }
            AssignmentAttributeWriteDiagnostic::CannotAssignToInstanceAttribute => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&INVALID_ATTRIBUTE_ACCESS, self.target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign to instance attribute `{}` from the class object `{}`",
                        self.attribute,
                        self.object_ty.display(db, env)
                    ));
                }
            }
        }
    }
}
