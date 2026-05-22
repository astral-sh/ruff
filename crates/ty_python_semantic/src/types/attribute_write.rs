use ty_module_resolver::KnownModule;

use super::call::CallArguments;
use super::{IntersectionType, KnownClass, Type, TypeQualifiers};
use crate::Db;
use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, builtins_symbol};

/// The operation required to write an attribute.
///
/// This representation is shared between real assignments and synthetic protocol
/// writes. It deliberately does not infer a value expression, emit diagnostics, or
/// decide whether assigning to a `Final` member is allowed: those decisions differ
/// between its two consumers.
pub(super) enum AttributeWriteRequirement<'db> {
    /// A write to a union must be valid for every union element.
    All {
        object_ty: Type<'db>,
        element_tys: &'db [Type<'db>],
    },
    /// A write to an intersection may target any positive intersection element.
    Any {
        object_ty: Type<'db>,
        intersection: IntersectionType<'db>,
    },
    /// A value may be assigned without an attribute-specific constraint.
    Unconstrained,
    /// The object type does not permit writes at all.
    CannotAssign,
    Module(Option<Type<'db>>),
    Instance(Type<'db>),
    Class {
        object_ty: Type<'db>,
        member: ClassAttributeWriteMember<'db>,
    },
}

pub(super) enum InstanceAttributeWriteMember<'db> {
    ClassVar,
    Explicit {
        member: ExplicitAttributeWriteRequirement<'db>,
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
    },
    Instance(FallbackAttributeWriteRequirement<'db>),
    SetAttr,
}

pub(super) enum ClassAttributeWriteMember<'db> {
    Explicit {
        member: ExplicitAttributeWriteRequirement<'db>,
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
    },
    ClassAttribute(FallbackAttributeWriteRequirement<'db>),
    Unresolved {
        has_instance_attribute: bool,
    },
}

pub(super) enum ExplicitAttributeWriteRequirement<'db> {
    Descriptor {
        descriptor_ty: Type<'db>,
        setter_ty: Type<'db>,
        qualifiers: TypeQualifiers,
    },
    AssignableTo {
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
    },
}

impl ExplicitAttributeWriteRequirement<'_> {
    pub(super) fn qualifiers(&self) -> TypeQualifiers {
        match self {
            Self::Descriptor { qualifiers, .. } | Self::AssignableTo { qualifiers, .. } => {
                *qualifiers
            }
        }
    }
}

pub(super) enum FallbackAttributeWriteRequirement<'db> {
    AssignableTo {
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
        possibly_missing: bool,
    },
    PossiblyMissing,
}

/// Resolve what writing `object_ty.attribute` would require.
pub(super) fn attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> AttributeWriteRequirement<'db> {
    match object_ty {
        Type::Union(union) => AttributeWriteRequirement::All {
            object_ty,
            element_tys: union.elements(db),
        },

        Type::Intersection(intersection) => {
            // TODO: Handle negative intersection elements.
            AttributeWriteRequirement::Any {
                object_ty,
                intersection,
            }
        }

        Type::EnumComplement(complement) => {
            attribute_write_requirement(db, complement.remaining_literal_union(db), attribute)
        }

        Type::TypeAlias(alias) => attribute_write_requirement(db, alias.value_type(db), attribute),

        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Super) => {
            AttributeWriteRequirement::CannotAssign
        }
        Type::BoundSuper(_) => AttributeWriteRequirement::CannotAssign,

        Type::Dynamic(..) | Type::Divergent(_) | Type::Never => {
            AttributeWriteRequirement::Unconstrained
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
        | Type::NewTypeInstance(_) => AttributeWriteRequirement::Instance(object_ty),

        Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
            class_attribute_write_requirement(db, object_ty, attribute)
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
            AttributeWriteRequirement::Module(match symbol.place {
                Place::Defined(DefinedPlace { ty, .. }) => Some(ty),
                Place::Undefined => None,
            })
        }
    }
}

pub(super) fn instance_attribute_write_member_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> InstanceAttributeWriteMember<'db> {
    let Some((meta_attr, fallback_attr)) = assignment_attribute_members(db, object_ty, attribute)
    else {
        return InstanceAttributeWriteMember::SetAttr;
    };

    match meta_attr {
        meta_attr @ PlaceAndQualifiers { .. } if meta_attr.is_class_var() => {
            InstanceAttributeWriteMember::ClassVar
        }
        PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace { ty, .. }),
            qualifiers,
        } => InstanceAttributeWriteMember::Explicit {
            member: explicit_attribute_write_requirement(
                db,
                object_ty,
                attribute,
                ty.bind_self_typevars(db, object_ty),
                qualifiers,
            ),
            fallback: fallback_attr.map(|fallback| {
                instance_fallback_write_requirement(db, object_ty, attribute, fallback)
            }),
        },
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => match fallback_attr {
            Some(
                fallback @ PlaceAndQualifiers {
                    place: Place::Defined(_),
                    ..
                },
            ) => InstanceAttributeWriteMember::Instance(instance_fallback_write_requirement(
                db, object_ty, attribute, fallback,
            )),
            _ => InstanceAttributeWriteMember::SetAttr,
        },
    }
}

fn class_attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> AttributeWriteRequirement<'db> {
    let Some((meta_attr, fallback_attr)) = assignment_attribute_members(db, object_ty, attribute)
    else {
        return AttributeWriteRequirement::Unconstrained;
    };
    let Some(class_attr_self_ty) = object_ty.to_instance(db) else {
        return AttributeWriteRequirement::Unconstrained;
    };

    let member = match meta_attr {
        PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace { ty, .. }),
            qualifiers,
        } => ClassAttributeWriteMember::Explicit {
            member: explicit_attribute_write_requirement(db, object_ty, attribute, ty, qualifiers),
            fallback: fallback_attr
                .map(|fallback| class_fallback_write_requirement(db, class_attr_self_ty, fallback)),
        },
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => match fallback_attr {
            Some(
                fallback @ PlaceAndQualifiers {
                    place: Place::Defined(_),
                    ..
                },
            ) => ClassAttributeWriteMember::ClassAttribute(class_fallback_write_requirement(
                db,
                class_attr_self_ty,
                fallback,
            )),
            _ => ClassAttributeWriteMember::Unresolved {
                has_instance_attribute: !class_attr_self_ty
                    .instance_member(db, attribute)
                    .place
                    .is_undefined(),
            },
        },
    };

    AttributeWriteRequirement::Class { object_ty, member }
}

fn explicit_attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    attr_ty: Type<'db>,
    qualifiers: TypeQualifiers,
) -> ExplicitAttributeWriteRequirement<'db> {
    if let Place::Defined(DefinedPlace { ty: setter_ty, .. }) =
        attr_ty.class_member(db, "__set__".into()).place
    {
        ExplicitAttributeWriteRequirement::Descriptor {
            descriptor_ty: attr_ty,
            setter_ty,
            qualifiers,
        }
    } else {
        ExplicitAttributeWriteRequirement::AssignableTo {
            ty: effective_write_type(db, object_ty, attribute, attr_ty),
            qualifiers,
        }
    }
}

fn instance_fallback_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    fallback: PlaceAndQualifiers<'db>,
) -> FallbackAttributeWriteRequirement<'db> {
    let PlaceAndQualifiers {
        place: Place::Defined(DefinedPlace {
            ty, definedness, ..
        }),
        qualifiers,
    } = fallback
    else {
        return FallbackAttributeWriteRequirement::PossiblyMissing;
    };
    let ty = ty.bind_self_typevars(db, object_ty);
    FallbackAttributeWriteRequirement::AssignableTo {
        ty: effective_write_type(db, object_ty, attribute, ty),
        qualifiers,
        possibly_missing: definedness == Definedness::PossiblyUndefined,
    }
}

fn class_fallback_write_requirement<'db>(
    db: &'db dyn Db,
    class_attr_self_ty: Type<'db>,
    fallback: PlaceAndQualifiers<'db>,
) -> FallbackAttributeWriteRequirement<'db> {
    let PlaceAndQualifiers {
        place: Place::Defined(DefinedPlace {
            ty, definedness, ..
        }),
        qualifiers,
    } = fallback
    else {
        return FallbackAttributeWriteRequirement::PossiblyMissing;
    };
    FallbackAttributeWriteRequirement::AssignableTo {
        ty: ty.bind_self_typevars(db, class_attr_self_ty),
        qualifiers,
        possibly_missing: definedness == Definedness::PossiblyUndefined,
    }
}

/// Return the accepted type for writes to an explicitly declared attribute.
///
/// A dataclass field with a converter accepts the converter's input type, not
/// the field's post-conversion type.
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

pub(super) fn property_setter_returns_never<'db>(
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

pub(super) fn assignment_attribute_members<'db>(
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
