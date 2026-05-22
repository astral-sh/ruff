use ty_module_resolver::KnownModule;

use super::call::{CallArguments, CallDunderError, CallError};
use super::{KnownClass, MemberLookupPolicy, Type, TypeContext, TypeQualifiers};
use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, builtins_symbol};

/// A diagnostic condition encountered while checking an attribute write.
///
/// The write traversal is shared by expression inference and synthetic protocol
/// writes. Only expression inference reports these conditions to the user.
pub(super) enum AttributeWriteDiagnostic<'db> {
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

/// Supplies the caller-specific parts of attribute-write validation.
///
/// Expression inference uses `bool` results and may infer the value repeatedly
/// with different type contexts. Protocol checks use `ConstraintSet` results
/// for a fixed synthetic value type.
pub(super) trait AttributeWriteVisitor<'db> {
    type Output: Copy;

    fn infer_value(&mut self, tcx: TypeContext<'db>, emit_diagnostics: bool) -> Type<'db>;

    fn infer_value_with_last_context(&mut self, emit_diagnostics: bool) -> Type<'db>;

    fn check_type_pair(
        &mut self,
        db: &'db dyn Db,
        value_ty: Type<'db>,
        target_ty: Type<'db>,
        emit_diagnostics: bool,
    ) -> Self::Output;

    fn constant(&self, value: bool) -> Self::Output;

    fn and(&self, db: &'db dyn Db, left: Self::Output, right: Self::Output) -> Self::Output;

    fn or(&self, db: &'db dyn Db, left: Self::Output, right: Self::Output) -> Self::Output;

    fn is_never(&self, db: &'db dyn Db, result: Self::Output) -> bool;

    fn is_always(&self, db: &'db dyn Db, result: Self::Output) -> bool;

    /// Validate writing a member qualified as `Final`.
    ///
    /// A real assignment can be a permitted initialization; a synthetic
    /// protocol write cannot write through a `Final` member.
    fn check_final(
        &mut self,
        object_ty: Type<'db>,
        qualifiers: TypeQualifiers,
        emit_diagnostics: bool,
    ) -> Self::Output;

    /// Report `Final` violations after a composite type has been accepted.
    ///
    /// Expression assignment checks defer this diagnostic until after selecting
    /// a valid union/intersection path. Synthetic writes reject individual
    /// `Final` members through `check_final` instead.
    fn check_composite_final(&mut self, object_ty: Type<'db>, emit_diagnostics: bool);

    fn report(&mut self, diagnostic: AttributeWriteDiagnostic<'db>);
}

/// Validate a write to `object_ty.attribute` using caller-specific type comparison behavior.
pub(super) fn validate_attribute_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    visitor: &mut V,
    emit_diagnostics: bool,
) -> V::Output {
    match object_ty {
        Type::Union(union) => {
            let value_ty = visitor.infer_value(TypeContext::default(), emit_diagnostics);
            let mut result = visitor.constant(true);
            for element in union.elements(db) {
                let element_result =
                    validate_attribute_write(db, *element, attribute, visitor, false);
                result = visitor.and(db, result, element_result);
                if visitor.is_never(db, result) {
                    break;
                }
            }
            if visitor.is_never(db, result) {
                if emit_diagnostics {
                    visitor.report(AttributeWriteDiagnostic::InvalidCompositeAssignment {
                        object_ty,
                        value_ty,
                    });
                }
                visitor.constant(false)
            } else {
                visitor.check_composite_final(object_ty, emit_diagnostics);
                result
            }
        }

        Type::Intersection(intersection) => {
            let mut result = visitor.constant(false);
            for element in intersection.positive(db) {
                let element_result =
                    validate_attribute_write(db, *element, attribute, visitor, false);
                result = visitor.or(db, result, element_result);
                if visitor.is_always(db, result) {
                    break;
                }
            }
            if visitor.is_never(db, result) {
                let value_ty = visitor.infer_value(TypeContext::default(), emit_diagnostics);
                if emit_diagnostics {
                    visitor.report(AttributeWriteDiagnostic::InvalidCompositeAssignment {
                        object_ty,
                        value_ty,
                    });
                }
                visitor.constant(false)
            } else {
                visitor.infer_value_with_last_context(emit_diagnostics);
                visitor.check_composite_final(object_ty, emit_diagnostics);
                result
            }
        }

        Type::EnumComplement(complement) => validate_attribute_write(
            db,
            complement.remaining_literal_union(db),
            attribute,
            visitor,
            emit_diagnostics,
        ),

        Type::TypeAlias(alias) => validate_attribute_write(
            db,
            alias.value_type(db),
            attribute,
            visitor,
            emit_diagnostics,
        ),

        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Super) => {
            visitor.infer_value(TypeContext::default(), emit_diagnostics);
            if emit_diagnostics {
                visitor.report(AttributeWriteDiagnostic::CannotAssign);
            }
            visitor.constant(false)
        }
        Type::BoundSuper(_) => {
            visitor.infer_value(TypeContext::default(), emit_diagnostics);
            if emit_diagnostics {
                visitor.report(AttributeWriteDiagnostic::CannotAssign);
            }
            visitor.constant(false)
        }

        Type::Dynamic(..) | Type::Divergent(_) | Type::Never => {
            visitor.infer_value(TypeContext::default(), emit_diagnostics);
            visitor.constant(true)
        }

        Type::NominalInstance(..)
        | Type::ProtocolInstance(_)
        | Type::LiteralValue(..)
        | Type::SpecialForm(..)
        | Type::KnownInstance(..)
        | Type::PropertyInstance(..)
        | Type::FunctionLiteral(..)
        | Type::Callable(..)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::DataclassDecorator(_)
        | Type::DataclassTransformer(_)
        | Type::TypeVar(..)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypedDict(_)
        | Type::NewTypeInstance(_) => {
            validate_instance_attribute_write(db, object_ty, attribute, visitor, emit_diagnostics)
        }

        Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
            validate_class_attribute_write(db, object_ty, attribute, visitor, emit_diagnostics)
        }

        Type::ModuleLiteral(module) => {
            let symbol = if module
                .module(db)
                .known(db)
                .is_some_and(KnownModule::is_builtins)
            {
                builtins_symbol(db, attribute)
            } else {
                module.static_member(db, attribute)
            };
            if let Place::Defined(DefinedPlace { ty: attr_ty, .. }) = symbol.place {
                let value_ty =
                    visitor.infer_value(TypeContext::new(Some(attr_ty)), emit_diagnostics);
                visitor.check_type_pair(db, value_ty, attr_ty, emit_diagnostics)
            } else {
                visitor.infer_value(TypeContext::default(), emit_diagnostics);
                if emit_diagnostics {
                    visitor.report(AttributeWriteDiagnostic::Unresolved { with_period: true });
                }
                visitor.constant(false)
            }
        }
    }
}

fn validate_instance_attribute_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    visitor: &mut V,
    emit_diagnostics: bool,
) -> V::Output {
    let value_ty = visitor.infer_value(TypeContext::default(), emit_diagnostics);
    let setattr_result = object_ty.try_call_dunder_with_policy(
        db,
        "__setattr__",
        &mut CallArguments::positional([Type::string_literal(db, attribute), value_ty]),
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
                "__setattr__".into(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            ) {
                PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace { ty: attr_ty, .. }),
                    ..
                } => attr_ty.is_callable_type(),
                _ => false,
            };
            let member_exists = !object_ty.member(db, attribute).place.is_undefined();
            visitor.report(AttributeWriteDiagnostic::TerminalSetAttr {
                member_exists,
                is_setattr_synthesized,
            });
        }
        return visitor.constant(false);
    }

    let Some((meta_attr, fallback_attr)) = assignment_attribute_members(db, object_ty, attribute)
    else {
        return visitor.constant(true);
    };

    match meta_attr {
        meta_attr @ PlaceAndQualifiers { .. } if meta_attr.is_class_var() => {
            if emit_diagnostics {
                visitor.report(AttributeWriteDiagnostic::CannotAssignToClassVar);
            }
            visitor.constant(false)
        }
        PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace {
                ty: meta_attr_ty, ..
            }),
            qualifiers,
        } => {
            let final_result = visitor.check_final(object_ty, qualifiers, emit_diagnostics);
            if visitor.is_never(db, final_result) {
                return final_result;
            }

            let meta_attr_ty = meta_attr_ty.bind_self_typevars(db, object_ty);
            let meta_result = check_explicit_attribute_write(
                db,
                object_ty,
                attribute,
                meta_attr_ty,
                value_ty,
                visitor,
                emit_diagnostics,
            );
            let Some(fallback_attr) = fallback_attr else {
                return visitor.and(db, final_result, meta_result);
            };
            let fallback_result = check_instance_fallback_write(
                db,
                object_ty,
                attribute,
                fallback_attr,
                visitor,
                emit_diagnostics,
            );
            visitor.and(
                db,
                final_result,
                visitor.and(db, meta_result, fallback_result),
            )
        }
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => {
            if let Some(
                fallback_attr @ PlaceAndQualifiers {
                    place: Place::Defined(_),
                    ..
                },
            ) = fallback_attr
            {
                check_instance_fallback_write(
                    db,
                    object_ty,
                    attribute,
                    fallback_attr,
                    visitor,
                    emit_diagnostics,
                )
            } else {
                match setattr_result {
                    Ok(_) | Err(CallDunderError::PossiblyUnbound { .. }) => visitor.constant(true),
                    Err(CallDunderError::CallError(..)) => {
                        if emit_diagnostics {
                            visitor.report(AttributeWriteDiagnostic::BadSetAttr { value_ty });
                        }
                        visitor.constant(false)
                    }
                    Err(CallDunderError::MethodNotAvailable) => {
                        if emit_diagnostics {
                            visitor.report(AttributeWriteDiagnostic::Unresolved {
                                with_period: false,
                            });
                        }
                        visitor.constant(false)
                    }
                }
            }
        }
    }
}

fn validate_class_attribute_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    visitor: &mut V,
    emit_diagnostics: bool,
) -> V::Output {
    let Some((meta_attr, fallback_attr)) = assignment_attribute_members(db, object_ty, attribute)
    else {
        visitor.infer_value(TypeContext::default(), emit_diagnostics);
        return visitor.constant(true);
    };
    let Some(class_attr_self_ty) = object_ty.to_instance(db) else {
        visitor.infer_value(TypeContext::default(), emit_diagnostics);
        return visitor.constant(true);
    };

    match meta_attr {
        PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace {
                ty: meta_attr_ty, ..
            }),
            qualifiers,
        } => {
            let final_result = visitor.check_final(object_ty, qualifiers, emit_diagnostics);
            if visitor.is_never(db, final_result) {
                visitor.infer_value(TypeContext::default(), emit_diagnostics);
                return final_result;
            }
            let value_ty = visitor.infer_value(TypeContext::default(), emit_diagnostics);
            let meta_result = check_explicit_attribute_write(
                db,
                object_ty,
                attribute,
                meta_attr_ty,
                value_ty,
                visitor,
                emit_diagnostics,
            );
            let Some(fallback_attr) = fallback_attr else {
                return visitor.and(db, final_result, meta_result);
            };
            let fallback_result = check_class_fallback_write(
                db,
                object_ty,
                class_attr_self_ty,
                fallback_attr,
                visitor,
                emit_diagnostics,
                false,
            );
            visitor.and(
                db,
                final_result,
                visitor.and(db, meta_result, fallback_result),
            )
        }
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => {
            if let Some(
                fallback_attr @ PlaceAndQualifiers {
                    place: Place::Defined(_),
                    ..
                },
            ) = fallback_attr
            {
                check_class_fallback_write(
                    db,
                    object_ty,
                    class_attr_self_ty,
                    fallback_attr,
                    visitor,
                    emit_diagnostics,
                    true,
                )
            } else {
                visitor.infer_value(TypeContext::default(), emit_diagnostics);
                if object_ty.to_instance(db).is_some_and(|instance| {
                    !instance.instance_member(db, attribute).place.is_undefined()
                }) {
                    if emit_diagnostics {
                        visitor.report(AttributeWriteDiagnostic::CannotAssignToInstanceAttribute);
                    }
                } else if emit_diagnostics {
                    visitor.report(AttributeWriteDiagnostic::Unresolved { with_period: true });
                }
                visitor.constant(false)
            }
        }
    }
}

fn check_explicit_attribute_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    attr_ty: Type<'db>,
    value_ty: Type<'db>,
    visitor: &mut V,
    emit_diagnostics: bool,
) -> V::Output {
    if let Place::Defined(DefinedPlace { ty: dunder_set, .. }) =
        attr_ty.class_member(db, "__set__".into()).place
    {
        let result = dunder_set.try_call(
            db,
            &CallArguments::positional([attr_ty, object_ty, value_ty]),
        );
        if property_setter_returns_never(db, attr_ty, object_ty, value_ty) {
            if emit_diagnostics {
                visitor.report(AttributeWriteDiagnostic::TerminalDescriptor);
            }
            return visitor.constant(false);
        }
        match result {
            Ok(_) => visitor.constant(true),
            Err(error) => {
                if emit_diagnostics {
                    visitor.report(AttributeWriteDiagnostic::BadDunderSet(error));
                }
                visitor.constant(false)
            }
        }
    } else {
        let write_ty = effective_write_type(db, object_ty, attribute, attr_ty);
        let value_ty = visitor.infer_value(TypeContext::new(Some(write_ty)), false);
        visitor.check_type_pair(db, value_ty, write_ty, emit_diagnostics)
    }
}

fn check_instance_fallback_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    fallback_attr: PlaceAndQualifiers<'db>,
    visitor: &mut V,
    emit_diagnostics: bool,
) -> V::Output {
    let PlaceAndQualifiers {
        place:
            Place::Defined(DefinedPlace {
                ty: instance_attr_ty,
                definedness,
                ..
            }),
        qualifiers,
    } = fallback_attr
    else {
        visitor.report(AttributeWriteDiagnostic::PossiblyMissing);
        return visitor.constant(true);
    };
    let final_result = visitor.check_final(object_ty, qualifiers, emit_diagnostics);
    if visitor.is_never(db, final_result) {
        return final_result;
    }
    let instance_attr_ty = instance_attr_ty.bind_self_typevars(db, object_ty);
    let write_ty = effective_write_type(db, object_ty, attribute, instance_attr_ty);
    let value_ty = visitor.infer_value(TypeContext::new(Some(write_ty)), false);
    let write_result = visitor.check_type_pair(db, value_ty, write_ty, emit_diagnostics);
    if definedness == Definedness::PossiblyUndefined {
        visitor.report(AttributeWriteDiagnostic::PossiblyMissing);
    }
    visitor.and(db, final_result, write_result)
}

fn check_class_fallback_write<'db, V: AttributeWriteVisitor<'db>>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    class_attr_self_ty: Type<'db>,
    fallback_attr: PlaceAndQualifiers<'db>,
    visitor: &mut V,
    emit_diagnostics: bool,
    is_primary_lookup: bool,
) -> V::Output {
    let PlaceAndQualifiers {
        place:
            Place::Defined(DefinedPlace {
                ty: class_attr_ty,
                definedness,
                ..
            }),
        qualifiers,
    } = fallback_attr
    else {
        visitor.report(AttributeWriteDiagnostic::PossiblyMissing);
        return visitor.constant(true);
    };
    let class_attr_ty = class_attr_ty.bind_self_typevars(db, class_attr_self_ty);
    let value_ty = visitor.infer_value(
        TypeContext::new(Some(class_attr_ty)),
        is_primary_lookup && emit_diagnostics,
    );
    let final_result = visitor.check_final(object_ty, qualifiers, emit_diagnostics);
    if visitor.is_never(db, final_result) {
        return final_result;
    }
    let write_result = visitor.check_type_pair(db, value_ty, class_attr_ty, emit_diagnostics);
    if definedness == Definedness::PossiblyUndefined {
        visitor.report(AttributeWriteDiagnostic::PossiblyMissing);
    }
    visitor.and(db, final_result, write_result)
}

fn effective_write_type<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    attr_ty: Type<'db>,
) -> Type<'db> {
    if let Type::NominalInstance(instance) = object_ty
        && let Some(converter_ty) = instance
            .class(db)
            .converter_input_type_for_field(db, attribute)
    {
        converter_ty
    } else {
        attr_ty
    }
}

fn property_setter_returns_never<'db>(
    db: &'db dyn Db,
    property_ty: Type<'db>,
    object_ty: Type<'db>,
    value_ty: Type<'db>,
) -> bool {
    property_ty.as_property_instance().is_some_and(|property| {
        property.setter(db).is_some_and(|setter| {
            match setter.try_call(db, &CallArguments::positional([object_ty, value_ty])) {
                Ok(result) => result.return_type(db).is_never(),
                Err(error) => error.return_type(db).is_never(),
            }
        })
    })
}

fn assignment_attribute_members<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> Option<(PlaceAndQualifiers<'db>, Option<PlaceAndQualifiers<'db>>)> {
    let meta_attr = object_ty.class_member(db, attribute.into());
    let needs_fallback = matches!(
        meta_attr.place,
        Place::Defined(DefinedPlace {
            definedness: Definedness::PossiblyUndefined,
            ..
        }) | Place::Undefined
    );
    let fallback_attr = if needs_fallback {
        Some(match object_ty {
            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::LiteralValue(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::EnumComplement(_)
            | Type::TypeVar(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => object_ty.instance_member(db, attribute),
            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => object_ty
                .find_name_in_mro(db, attribute)
                .expect("called on Type::ClassLiteral, Type::GenericAlias, or Type::SubclassOf"),
            Type::Union(..)
            | Type::Intersection(..)
            | Type::TypeAlias(..)
            | Type::Dynamic(..)
            | Type::Divergent(_)
            | Type::Never
            | Type::ModuleLiteral(..)
            | Type::BoundSuper(..) => return None,
        })
    } else {
        None
    };
    Some((meta_attr, fallback_attr))
}
