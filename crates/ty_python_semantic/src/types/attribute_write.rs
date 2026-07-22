//! Attribute-write resolution shared by assignment inference and protocol compatibility.
//!
//! This module resolves the Python lookup semantics for `object.attribute = value` into an
//! [`AttributeWriteRequirement`]. The requirement retains alternatives such as union elements,
//! descriptors, instance fallbacks, and `__setattr__` instead of deciding whether a particular
//! value type is valid. Assignment inference can therefore provide contextual inference and
//! diagnostics, while protocol checking can evaluate the same lookup result using its active type
//! relation and constraint set.

use ty_module_resolver::KnownModule;

use super::call::CallArguments;
use super::callable::CallableTypeKind;
use super::{
    IntersectionType, KnownClass, KnownInstanceType, MemberLookupPolicy, Type, TypeQualifiers,
};
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
    /// A module symbol, with its declared type when the symbol is known.
    ///
    /// `None` represents an unresolved module attribute rather than an unconstrained write.
    Module(Option<Type<'db>>),
    /// The effective instance-write requirement of a declared protocol member.
    ///
    /// `write` is `None` for a read-only member. Qualifiers are retained so assignment inference
    /// can distinguish `Final` and `ClassVar` diagnostics from other non-writable members.
    ProtocolMember {
        write: Option<ProtocolMemberWriteRequirement<'db>>,
        qualifiers: TypeQualifiers,
    },
    /// A write through an instance, resolved against its class and instance attributes.
    Instance {
        object_ty: Type<'db>,
        member: InstanceAttributeWriteMember<'db>,
    },
    /// A write through a class object, resolved against its metaclass and class attributes.
    Class {
        object_ty: Type<'db>,
        member: ClassAttributeWriteMember<'db>,
    },
}

/// How a writable protocol member validates an assigned value.
pub(super) enum ProtocolMemberWriteRequirement<'db> {
    /// Check the assigned value against a directly representable write type.
    AssignableTo(Type<'db>),
    /// Invoke every possible descriptor setter with the assigned value.
    ///
    /// `domain` is the precisely derived write type when that domain fits in [`Type`]. It is used
    /// for contextual inference and protocol compatibility, while descriptor calls remain the
    /// authority for real assignments. `None` preserves a known write capability whose generic or
    /// set-theoretic domain cannot be represented precisely.
    Descriptor {
        descriptor_ty: Type<'db>,
        receiver_ty: Type<'db>,
        domain: Option<Type<'db>>,
    },
}

/// The member that governs a write through an instance.
///
/// A declared class member takes precedence over an instance fallback. A custom `__setattr__` is
/// used only when neither lookup produces a declared write target; callers separately account for
/// a terminal `__setattr__` that blocks every write.
pub(super) enum InstanceAttributeWriteMember<'db> {
    /// The resolved declaration is a `ClassVar`, which cannot be assigned through an instance.
    ClassVar,
    /// A class or MRO member governs the write.
    ///
    /// The fallback is also required when the explicit member is only possibly defined.
    Explicit {
        member: ExplicitAttributeWriteRequirement<'db>,
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
    },
    /// No class member governs the write, but an instance declaration does.
    Instance(FallbackAttributeWriteRequirement<'db>),
    /// No declared member was found, so the write is governed by `__setattr__`.
    SetAttr,
}

/// The member that governs a write through a class object.
///
/// A data descriptor on the metaclass takes precedence over the class object's own attributes,
/// which in turn take precedence over definitely non-data metaclass members. If the metaclass
/// member is absent, possibly undefined, or could be a non-data descriptor, the class object's own
/// attributes form the fallback.
pub(super) enum ClassAttributeWriteMember<'db> {
    /// A metaclass member governs the write, optionally alongside a class-attribute fallback.
    Explicit {
        member: ExplicitAttributeWriteRequirement<'db>,
        fallback: Option<FallbackAttributeWriteRequirement<'db>>,
    },
    /// No metaclass member governs the write, but a class attribute does.
    ClassAttribute(FallbackAttributeWriteRequirement<'db>),
    /// Neither lookup found a writable declaration.
    ///
    /// `has_instance_attribute` distinguishes assigning an instance-only declaration through the
    /// class from assigning a wholly unknown attribute.
    Unresolved { has_instance_attribute: bool },
}

/// How an explicitly resolved member accepts a write.
pub(super) enum ExplicitAttributeWriteRequirement<'db> {
    /// Invoke a concrete descriptor's `__set__` method.
    ///
    /// `setter_ty` is the unbound method and is called with `descriptor_ty`, the object, and the
    /// assigned value.
    Descriptor {
        descriptor_ty: Type<'db>,
        setter_ty: Type<'db>,
        qualifiers: TypeQualifiers,
    },
    /// Check the assigned value directly against the member's effective write type.
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

/// A receiver-level write target that can govern the write instead of the type member.
pub(super) enum FallbackAttributeWriteRequirement<'db> {
    /// Check the value against `ty`, retaining whether the declaration may be absent at runtime.
    AssignableTo {
        ty: Type<'db>,
        qualifiers: TypeQualifiers,
        possibly_missing: bool,
    },
    /// The fallback may exist, but lookup did not produce a usable write type.
    PossiblyMissing,
}

/// The members that can govern an attribute write.
///
/// For a class-object receiver, the type member is found on the metaclass while the receiver member
/// is found on the class's own MRO:
///
/// ```python
/// class Descriptor:
///     def __set__(self, instance: object, value: object) -> None: ...
///
/// class Meta(type):
///     data = Descriptor()  # Type member: Meta.data
///     plain = object()  # Type member: Meta.plain
///
/// class C(metaclass=Meta):
///     data: int  # Receiver member: C.data
///     plain: int  # Receiver member: C.plain
///
/// C.data = 1
/// C.plain = 1
/// ```
pub(super) enum AssignmentAttributeMembers<'db> {
    /// The type member governs the write, as `Meta.data` does above because it is a data descriptor.
    /// If the type member may be missing or may be a non-data descriptor, the corresponding
    /// receiver member (`C.data`) is retained as `receiver_fallback`.
    TypeMember {
        member: PlaceAndQualifiers<'db>,
        receiver_fallback: Option<PlaceAndQualifiers<'db>>,
    },
    /// The receiver member governs the write, as `C.plain` does above because `Meta.plain` is
    /// definitely not a data descriptor.
    ReceiverMember(PlaceAndQualifiers<'db>),
}

impl<'db> AssignmentAttributeMembers<'db> {
    /// Return the member whose descriptor protocol applies to the receiver, if any.
    pub(super) fn type_member(self) -> Option<PlaceAndQualifiers<'db>> {
        match self {
            Self::TypeMember { member, .. } => Some(member),
            Self::ReceiverMember(_) => None,
        }
    }

    /// Iterate over every member that can govern the write at runtime.
    pub(super) fn effective_members(self) -> impl Iterator<Item = PlaceAndQualifiers<'db>> {
        let members = match self {
            Self::TypeMember {
                member,
                receiver_fallback,
            } => [Some(member), receiver_fallback],
            Self::ReceiverMember(member) => [Some(member), None],
        };
        members.into_iter().flatten()
    }
}

/// Resolve the receiver-level requirements for writing `object_ty.attribute`.
///
/// This expands aliases, preserves the all-arms rule for unions and the any-positive-arm rule for
/// intersections, and dispatches instance and class-object writes to their respective lookup
/// paths. It does not compare the assigned value with the resulting types.
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

        Type::ProtocolInstance(protocol) => protocol
            .interface(db)
            .instance_write_requirement(db, object_ty, attribute)
            .map_or_else(
                || instance_attribute_write_requirement(db, object_ty, attribute),
                |(write, qualifiers)| AttributeWriteRequirement::ProtocolMember {
                    write,
                    qualifiers,
                },
            ),

        Type::NominalInstance(..)
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
        | Type::TypeForm(_)
        | Type::TypedDict(_)
        | Type::NewTypeInstance(_) => {
            instance_attribute_write_requirement(db, object_ty, attribute)
        }

        Type::SubclassOf(subclass_of) => subclass_of
            .meta_write_requirement(db, attribute)
            .map_or_else(
                || class_attribute_write_requirement(db, object_ty, attribute),
                |(write_ty, qualifiers)| AttributeWriteRequirement::ProtocolMember {
                    write: write_ty.map(ProtocolMemberWriteRequirement::AssignableTo),
                    qualifiers,
                },
            ),

        Type::ClassLiteral(..) | Type::GenericAlias(..) => {
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

fn instance_attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> AttributeWriteRequirement<'db> {
    AttributeWriteRequirement::Instance {
        object_ty,
        member: instance_attribute_write_member_requirement(db, object_ty, attribute),
    }
}

/// Resolve the declared member that governs an instance attribute write.
///
/// The returned requirement preserves a possibly-defined class member and its instance fallback,
/// because both runtime paths must accept the write. If neither exists, the caller must evaluate
/// `__setattr__`.
fn instance_attribute_write_member_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> InstanceAttributeWriteMember<'db> {
    let Some(members) = assignment_attribute_members(db, object_ty, attribute) else {
        return InstanceAttributeWriteMember::SetAttr;
    };
    let (type_member, receiver_fallback) = match members {
        AssignmentAttributeMembers::TypeMember {
            member,
            receiver_fallback,
        } => (member, receiver_fallback),
        AssignmentAttributeMembers::ReceiverMember(member) => {
            return InstanceAttributeWriteMember::Instance(instance_fallback_write_requirement(
                db, object_ty, attribute, member,
            ));
        }
    };

    match type_member {
        type_member if type_member.is_class_var() => InstanceAttributeWriteMember::ClassVar,
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
            fallback: receiver_fallback.map(|fallback| {
                instance_fallback_write_requirement(db, object_ty, attribute, fallback)
            }),
        },
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => match receiver_fallback {
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

/// Resolve a class-object write against the metaclass and then the class object's attributes.
///
/// The receiver must be convertible to an instance type so that `Self` in class-attribute
/// declarations can be bound consistently with normal class-object member lookup.
fn class_attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> AttributeWriteRequirement<'db> {
    let Some(members) = assignment_attribute_members(db, object_ty, attribute) else {
        return AttributeWriteRequirement::Unconstrained;
    };
    let Some(class_attr_self_ty) = object_ty.to_instance_approximation(db) else {
        return AttributeWriteRequirement::Unconstrained;
    };
    let (type_member, receiver_fallback) = match members {
        AssignmentAttributeMembers::TypeMember {
            member,
            receiver_fallback,
        } => (member, receiver_fallback),
        AssignmentAttributeMembers::ReceiverMember(member) => {
            return AttributeWriteRequirement::Class {
                object_ty,
                member: ClassAttributeWriteMember::ClassAttribute(
                    class_fallback_write_requirement(db, object_ty, class_attr_self_ty, member),
                ),
            };
        }
    };

    let member = match type_member {
        PlaceAndQualifiers {
            place: Place::Defined(DefinedPlace { ty, .. }),
            qualifiers,
        } => ClassAttributeWriteMember::Explicit {
            member: explicit_attribute_write_requirement(db, object_ty, attribute, ty, qualifiers),
            fallback: receiver_fallback.map(|fallback| {
                class_fallback_write_requirement(db, object_ty, class_attr_self_ty, fallback)
            }),
        },
        PlaceAndQualifiers {
            place: Place::Undefined,
            ..
        } => match receiver_fallback {
            Some(
                fallback @ PlaceAndQualifiers {
                    place: Place::Defined(_),
                    ..
                },
            ) => ClassAttributeWriteMember::ClassAttribute(class_fallback_write_requirement(
                db,
                object_ty,
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

/// Convert an explicitly resolved member into either a descriptor call or a direct type check.
///
/// Descriptor behavior is used only when `__set__` is found with
/// [`MemberLookupPolicy::REQUIRE_CONCRETE`]. An `Any` or `Unknown` base therefore does not cause an
/// ordinary attribute to be treated as a data descriptor.
fn explicit_attribute_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    attr_ty: Type<'db>,
    qualifiers: TypeQualifiers,
) -> ExplicitAttributeWriteRequirement<'db> {
    if let Place::Defined(DefinedPlace { ty: setter_ty, .. }) = attr_ty
        .class_member_with_policy(db, "__set__", MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
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

/// Convert an instance fallback into a write type, binding `Self` to the receiver.
///
/// This also applies dataclass converter semantics and preserves possible undefinedness for the
/// assignment diagnostic layer.
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

/// Convert a class-attribute fallback into a write type, binding `Self` to the class instance.
fn class_fallback_write_requirement<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
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
    let ty = ty.bind_self_typevars(db, class_attr_self_ty);
    let ty = if matches!(object_ty, Type::ClassLiteral(_))
        && let Type::FunctionLiteral(function) = ty
        && function.callable_type_kind(db) == CallableTypeKind::FunctionLike
    {
        Type::Callable(function.into_callable_type(db))
    } else {
        ty
    };
    FallbackAttributeWriteRequirement::AssignableTo {
        ty,
        qualifiers,
        possibly_missing: definedness == Definedness::PossiblyUndefined,
    }
}

/// Return the accepted type for writes to a declared attribute.
///
/// A dataclass field with a converter accepts the converter's input type, not
/// the field's post-conversion type. For example, a field declared as `int` with a
/// `(str) -> int` converter is read as `int` but accepts `str` assignments.
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

/// Return whether a property setter is terminal for this receiver and value type.
///
/// This is intentionally specific to `property`: other descriptors are governed by the result of
/// their concrete `__set__` call. Both successful and failed call bindings retain the declared
/// return type, so either can establish that the selected setter returns `Never`/`NoReturn`.
///
/// ```python
/// from typing import Never
///
/// class Model:
///     @property
///     def value(self) -> int: ...
///
///     @value.setter
///     def value(self, value: int) -> Never: ...
/// ```
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

enum ClassObjectMemberPrecedence<'db> {
    /// The class-object member always shadows the metaclass member.
    Receiver(PlaceAndQualifiers<'db>),
    /// The metaclass member's descriptor status is uncertain, so either member can govern the
    /// write.
    TypeOrReceiver(PlaceAndQualifiers<'db>),
}

/// Classify the precedence between a metaclass member and a class-object member.
///
/// For example, `C.attribute` governs the assignment below because `Meta.attribute` does not
/// implement `__set__` or `__delete__`:
///
/// ```python
/// class Meta(type):
///     attribute = object()
///
/// class C(metaclass=Meta):
///     attribute: int
///
/// C.attribute = 1
/// ```
fn class_object_member_precedence<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
    type_member: PlaceAndQualifiers<'db>,
) -> Option<ClassObjectMemberPrecedence<'db>> {
    if !matches!(
        object_ty,
        Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..)
    ) {
        return None;
    }

    let receiver_member = object_ty
        .find_name_in_mro_with_policy(db, attribute, MemberLookupPolicy::default())
        .filter(|class_attr| !class_attr.place.is_undefined())?;
    let type_member_ty = type_member.place.ignore_possibly_undefined()?;

    if type_member_ty.is_definitely_non_data_descriptor(db) {
        Some(ClassObjectMemberPrecedence::Receiver(receiver_member))
    } else if !type_member_ty.is_divergent() && !type_member_ty.is_data_descriptor(db) {
        Some(ClassObjectMemberPrecedence::TypeOrReceiver(receiver_member))
    } else {
        None
    }
}

/// Return the members considered by attribute assignment in lookup-precedence order.
///
/// The type member comes from class-member lookup. A member found directly on the receiver is
/// queried when the type member is absent or possibly undefined. For class objects, a class-MRO
/// member instead takes precedence over a definitely non-data metaclass member and remains an
/// alternative when the metaclass member's descriptor status is uncertain. Composite and dynamic
/// receiver types return `None`; their callers either decompose them before this point or handle
/// them without member lookup.
///
/// This helper deliberately does not bind `Self` or interpret descriptors so that assignment,
/// protocol compatibility, and `Final` validation share exactly the same lookup precedence.
pub(super) fn assignment_attribute_members<'db>(
    db: &'db dyn Db,
    object_ty: Type<'db>,
    attribute: &str,
) -> Option<AssignmentAttributeMembers<'db>> {
    // Precise `functools.partial` instances synthesize a refined `__call__` member instead of
    // using the broad signature from typeshed.
    let type_member = if attribute == "__call__"
        && matches!(
            object_ty,
            Type::KnownInstance(KnownInstanceType::FunctoolsPartial(_))
        ) {
        object_ty.member(db, attribute)
    } else {
        object_ty.class_member(db, attribute)
    };
    let receiver_alternative =
        match class_object_member_precedence(db, object_ty, attribute, type_member) {
            Some(ClassObjectMemberPrecedence::Receiver(receiver_member)) => {
                return Some(AssignmentAttributeMembers::ReceiverMember(receiver_member));
            }
            Some(ClassObjectMemberPrecedence::TypeOrReceiver(receiver_member)) => {
                Some(receiver_member)
            }
            None => None,
        };
    let needs_receiver_fallback = matches!(
        type_member.place,
        Place::Defined(DefinedPlace {
            definedness: Definedness::PossiblyUndefined,
            ..
        }) | Place::Undefined
    );
    let receiver_fallback = if let Some(receiver_alternative) = receiver_alternative {
        Some(receiver_alternative)
    } else if needs_receiver_fallback {
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
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => object_ty.instance_member(db, attribute),
            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                object_ty.class_object_member(db, attribute, MemberLookupPolicy::default())
            }
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
    Some(AssignmentAttributeMembers::TypeMember {
        member: type_member,
        receiver_fallback,
    })
}
