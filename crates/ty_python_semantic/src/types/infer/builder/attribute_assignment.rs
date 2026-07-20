use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use super::{ArgumentsIter, MultiInferenceGuard, TypeInferenceBuilder};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers};
use crate::types::attribute_write::{
    AttributeWriteRequirement, ClassAttributeWriteMember, ExplicitAttributeWriteRequirement,
    FallbackAttributeWriteRequirement, InstanceAttributeWriteMember,
    ProtocolMemberWriteRequirement, attribute_write_requirement, property_setter_returns_never,
};
use crate::types::call::{CallArguments, CallError};
use crate::types::diagnostic::{
    INVALID_ASSIGNMENT, INVALID_ATTRIBUTE_ACCESS, UNRESOLVED_ATTRIBUTE, report_bad_dunder_set_call,
    report_invalid_attribute_assignment, report_possibly_missing_attribute,
};
use crate::types::{CallDunderError, MemberLookupPolicy, Type, TypeContext, TypeQualifiers};

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
        let requirement = attribute_write_requirement(self.db(), object_ty, attribute);
        let mut evaluator = AssignmentAttributeWriteEvaluator {
            builder: self,
            target,
            value,
            object_ty,
            attribute,
            infer_value_ty: MultiInferenceGuard::new(infer_value_ty),
        };
        evaluator.evaluate(&requirement, emit_diagnostics)
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
    BadDunderSet(CallError<'db>),
    PossiblyMissing,
    BadSetAttr {
        value_ty: Type<'db>,
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

    fn evaluate(
        &mut self,
        requirement: &AttributeWriteRequirement<'db>,
        emit_diagnostics: bool,
    ) -> bool {
        match requirement {
            AttributeWriteRequirement::All {
                object_ty,
                element_tys,
            } => {
                let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);
                let mut valid = true;
                for element_ty in *element_tys {
                    let requirement =
                        attribute_write_requirement(self.builder.db(), *element_ty, self.attribute);
                    if !self.evaluate(&requirement, false) {
                        valid = false;
                        break;
                    }
                }
                if valid {
                    self.validate_composite_final_assignment(*object_ty, emit_diagnostics);
                    true
                } else {
                    if emit_diagnostics {
                        self.report(
                            AssignmentAttributeWriteDiagnostic::InvalidCompositeAssignment {
                                object_ty: *object_ty,
                                value_ty,
                            },
                        );
                    }
                    false
                }
            }
            AttributeWriteRequirement::Any {
                object_ty,
                intersection,
            } => {
                let mut valid = false;
                for element_ty in intersection.positive(self.builder.db()) {
                    let requirement =
                        attribute_write_requirement(self.builder.db(), *element_ty, self.attribute);
                    if self.evaluate(&requirement, false) {
                        valid = true;
                        break;
                    }
                }
                if valid {
                    self.infer_with_last_context(emit_diagnostics);
                    self.validate_composite_final_assignment(*object_ty, emit_diagnostics);
                    true
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
                    false
                }
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
        let assignable = value_ty.is_assignable_to(db, target_ty);
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
        let value_ty = self.infer_value(TypeContext::default(), emit_diagnostics);

        // A terminal `__setattr__` blocks even explicitly declared attributes.
        let setattr_result = object_ty.try_call_dunder_with_policy(
            db,
            "__setattr__",
            &mut CallArguments::positional([Type::string_literal(db, self.attribute), value_ty]),
            TypeContext::default(),
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
        );
        let setattr_returns_never = match &setattr_result {
            Ok(bindings) => bindings.return_type(db).is_never(),
            Err(error) => error.return_type(db).is_some_and(|ty| ty.is_never()),
        };
        if setattr_returns_never {
            if emit_diagnostics {
                let is_setattr_synthesized = match object_ty.class_member_with_policy(
                    db,
                    "__setattr__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                ) {
                    PlaceAndQualifiers {
                        place: Place::Defined(DefinedPlace { ty, .. }),
                        ..
                    } => ty.is_callable_type(),
                    _ => false,
                };
                let member_exists = !object_ty.member(db, self.attribute).place.is_undefined();
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
                Err(CallDunderError::CallError(..)) => {
                    if emit_diagnostics {
                        self.report(AssignmentAttributeWriteDiagnostic::BadSetAttr { value_ty });
                    }
                    false
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
                let db = self.builder.db();
                let name_ty = Type::string_literal(db, self.attribute);
                let ast_arguments = [
                    ast::ArgOrKeyword::Arg(self.target.value.as_ref()),
                    ast::ArgOrKeyword::Arg(self.value),
                ];
                let mut call_arguments = CallArguments::positional([name_ty, Type::unknown()]);
                let setattr_result = self.builder.infer_and_try_call_dunder(
                    db,
                    object_ty,
                    "__setattr__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                        | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
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
                let setattr_returns_never = match &setattr_result {
                    Ok(bindings) => bindings.return_type(db).is_never(),
                    Err(error) => error.return_type(db).is_some_and(|ty| ty.is_never()),
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
                    Err(CallDunderError::CallError(..)) => {
                        if emit_diagnostics {
                            self.report(AssignmentAttributeWriteDiagnostic::BadSetAttr {
                                value_ty,
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

        if property_setter_returns_never(db, descriptor_ty, receiver_ty, value_ty) {
            if emit_diagnostics {
                self.report(AssignmentAttributeWriteDiagnostic::TerminalDescriptor);
            }
            return false;
        }

        match descriptor_ty.try_call_dunder_with_policy(
            db,
            "__set__",
            &mut CallArguments::positional([receiver_ty, value_ty]),
            TypeContext::default(),
            MemberLookupPolicy::REQUIRE_CONCRETE,
        ) {
            Ok(_) => true,
            Err(CallDunderError::CallError(kind, bindings, _)) => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::BadDunderSet(CallError(
                        kind, bindings,
                    )));
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
        if property_setter_returns_never(db, descriptor_ty, object_ty, value_ty) {
            if emit_diagnostics {
                self.report(AssignmentAttributeWriteDiagnostic::TerminalDescriptor);
            }
            return false;
        }

        match setter_ty.try_call(
            db,
            &CallArguments::positional([descriptor_ty, object_ty, value_ty]),
        ) {
            Ok(_) => true,
            Err(error) => {
                if emit_diagnostics {
                    self.report(AssignmentAttributeWriteDiagnostic::BadDunderSet(error));
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
                    builder.into_diagnostic(format_args!(
                        "Object of type `{}` is not assignable to attribute `{}` on type `{}`",
                        value_ty.display(db),
                        self.attribute,
                        object_ty.display(db),
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
                        self.object_ty.display(db),
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
                        self.object_ty.display(db),
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
                            self.object_ty.display(db)
                        )
                    } else if is_setattr_synthesized {
                        format!(
                            "Property `{}` defined in `{}` is read-only",
                            self.attribute,
                            self.object_ty.display(db)
                        )
                    } else {
                        format!(
                            "Cannot assign to attribute `{}` on type `{}` whose `__setattr__` method returns `Never`/`NoReturn`",
                            self.attribute,
                            self.object_ty.display(db)
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
                        self.object_ty.display(db),
                    ));
                }
            }
            AssignmentAttributeWriteDiagnostic::BadDunderSet(failure) => {
                report_bad_dunder_set_call(
                    &self.builder.context,
                    &failure,
                    self.attribute,
                    self.object_ty,
                    self.target,
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
            AssignmentAttributeWriteDiagnostic::BadSetAttr { value_ty } => {
                if let Some(builder) = self
                    .builder
                    .context
                    .report_lint(&UNRESOLVED_ATTRIBUTE, self.target)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot assign object of type `{}` to attribute `{}` on type `{}` with custom `__setattr__` method.",
                        value_ty.display(db),
                        self.attribute,
                        self.object_ty.display(db)
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
                            self.object_ty.display(db)
                        ));
                    } else {
                        builder.into_diagnostic(format_args!(
                            "Unresolved attribute `{}` on type `{}`",
                            self.attribute,
                            self.object_ty.display(db)
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
                        self.object_ty.display(db)
                    ));
                }
            }
        }
    }
}
