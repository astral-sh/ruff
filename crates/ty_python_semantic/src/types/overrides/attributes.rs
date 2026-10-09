//! Compatibility of the values read from and written to inherited attributes.

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;

use crate::{
    Db, ProgramEnvironment,
    place::{Place, TypeOrigin},
    types::{
        ClassBase, ClassType, InstanceFallbackShadowsNonDataDescriptor, IntersectionType,
        KnownInstanceType, MemberLookupPolicy, Type, TypeQualifiers,
        attribute_write::{DescriptorSetterDomain, descriptor_setter_domain},
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_ATTRIBUTE_OVERRIDE, INVALID_MUTABLE_OVERRIDE, INVALID_PROPERTY_TYPE_OVERRIDE,
        },
    },
};

/// The instance operations promised by one attribute declaration.
///
/// A descriptor can accept a different type from the one it returns. Ordinary mutable
/// attributes use the same type for both operations, making their types invariant.
struct AttributeContract<'db> {
    read: Type<'db>,
    write: Option<Type<'db>>,
    is_property: bool,
    is_method: bool,
    qualifiers: TypeQualifiers,
}

/// Resolve an owner's declaration as seen through the receiver being checked.
///
/// Keep the owner's generic specialization, but bind `Self` and descriptor access to
/// `receiver`. Looking up the name directly on the receiver would hide an overridden
/// declaration before its contract could be compared. Fields handled by dedicated
/// override rules return `None`.
///
/// ```python
/// class Base:
///     value: int
///
/// class Child(Base):
///     value: str
/// ```
///
/// With `owner = Base` and a `Child` receiver, the read contract remains `int`.
fn attribute_contract<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    owner: ClassType<'db>,
    receiver: Type<'db>,
    name: &str,
) -> Option<AttributeContract<'db>> {
    // `object.__class__` is specialized by member lookup, so its synthetic `Self` is
    // not an override contract. Likewise, not every object is hashable or has a
    // writable instance dictionary, despite the broad declarations in typeshed.
    if owner.is_object(db) && matches!(name, "__class__" | "__hash__" | "__dict__") {
        return None;
    }
    let (literal, _) = owner.static_class_literal(db)?;
    // NamedTuple fields have a dedicated override rule, including synthesized properties.
    if CodeGeneratorKind::NamedTuple.matches(db, literal.into())
        && literal
            .own_fields(db, None, CodeGeneratorKind::NamedTuple)
            .contains_key(name)
    {
        return None;
    }
    let class_member = owner.own_class_member(db, env, None, name).inner;
    let instance_member = owner.own_instance_member(db, env, name).inner;
    let Place::Defined(class_place) = class_member.place else {
        return None;
    };
    if matches!(class_place.ty, Type::TypeAlias(_)) {
        return None;
    }
    let alternatives = class_place
        .ty
        .as_union()
        .map_or(std::slice::from_ref(&class_place.ty), |union| {
            union.elements(db)
        });
    // Only pairs of methods go to the method checker. A decorator can turn a
    // function into a property, in which case its exposed value must be checked.
    let is_method = alternatives.iter().all(|ty| {
        matches!(ty, Type::FunctionLiteral(_))
            || matches!(ty, Type::Callable(callable) if callable.is_method_like(db))
    });
    let qualifiers = class_member.qualifiers | instance_member.qualifiers;
    let is_final = qualifiers.contains(TypeQualifiers::FINAL);
    let is_class_var = qualifiers.contains(TypeQualifiers::CLASS_VAR);
    let is_property = alternatives.iter().any(Type::is_property_instance);
    let is_slot = matches!(class_place.ty, Type::SlotDescriptor(_));
    let is_descriptor = !is_class_var
        && (is_slot
            || class_place
                .ty
                .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
                .place
                .ignore_possibly_undefined()
                .is_some());
    // Unannotated defaults with an inherited annotation already have a declared type.
    // Other inferred bindings do not define an independent write contract: their raw
    // types can retain literals that ordinary attribute access widens.
    if class_place.origin == TypeOrigin::Inferred && !is_descriptor {
        return None;
    }
    let (read, write) = if is_descriptor {
        let read = Type::resolve_descriptor_access(
            db,
            env,
            class_member,
            receiver,
            instance_member.into(),
            InstanceFallbackShadowsNonDataDescriptor::No,
        )
        .ok()?
        .member(db)
        .place
        .ignore_possibly_undefined()?
        .bind_self_typevars(db, env, receiver);
        // Explicit `staticmethod(f)` and `classmethod(f)` assignments expose method
        // signatures, just like decorated definitions; the function's identity can change.
        let read = if is_method
            || matches!(
                class_place.ty,
                Type::KnownInstance(KnownInstanceType::MethodWrapper(_))
            ) {
            read.try_upcast_to_callable(db, env)?.to_type(db, env)
        } else {
            read
        };
        let write = if is_slot {
            Some(
                owner
                    .converter_input_type_for_field(db, name)
                    .unwrap_or(read),
            )
        } else {
            descriptor_write_domain(db, env, class_place.ty, receiver, read)
        };
        (read, write)
    } else {
        // Inferred non-descriptors were excluded above, so the class member carries
        // the declared contract, including inherited annotations. Instance assignments
        // do not narrow that contract, even when a class-body default is present.
        let read = class_place.ty.bind_self_typevars(db, env, receiver);
        (
            read,
            Some(
                owner
                    .converter_input_type_for_field(db, name)
                    .unwrap_or(read),
            ),
        )
    };
    Some(AttributeContract {
        read,
        write: if is_final || !is_class_var && literal.is_frozen_dataclass(db) == Some(true) {
            None
        } else {
            write
        },
        is_property,
        is_method,
        qualifiers,
    })
}

/// Return the input type accepted by every possible descriptor or instance-storage path.
///
/// Union alternatives require the intersection of their accepted types. A non-data
/// descriptor can be shadowed by instance storage; a read-only data descriptor cannot.
/// `None` means writes are unavailable, while `Some(Unknown)` defers checks for setters
/// whose input domain cannot yet be represented.
fn descriptor_write_domain<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    descriptor: Type<'db>,
    receiver: Type<'db>,
    storage_type: Type<'db>,
) -> Option<Type<'db>> {
    let descriptor = descriptor.resolve_type_alias(db);
    if let Type::Union(union) = descriptor {
        let domains = union
            .elements(db)
            .iter()
            .map(|element| descriptor_write_domain(db, env, *element, receiver, storage_type))
            .collect::<Option<Vec<_>>>()?;
        return Some(
            IntersectionType::bounded_from_elements(db, env, domains).unwrap_or_else(Type::unknown),
        );
    }
    match descriptor_setter_domain(db, env, descriptor, receiver) {
        DescriptorSetterDomain::Known(ty) => Some(ty),
        DescriptorSetterDomain::Deferred => Some(Type::unknown()),
        DescriptorSetterDomain::Missing
            if descriptor.as_property_instance().is_some()
                || descriptor.is_data_descriptor(db, env) =>
        {
            None
        }
        DescriptorSetterDomain::Missing => Some(storage_type),
    }
}

enum AttributeViolation<'db> {
    Read {
        source: Type<'db>,
        target: Type<'db>,
    },
    Write {
        target: Type<'db>,
    },
    ReadOnly,
}

/// Find the first read or write that the overriding contract fails to preserve.
///
/// Both contracts are bound to the subclass receiver. `target_receiver` is the base
/// instance used to verify that a promised write is actually supported there; rejecting
/// an override for a write already rejected by the base would be a false positive.
/// Read types are covariant and write types are contravariant.
fn attribute_violation<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Type<'db>,
    target_receiver: Type<'db>,
    name: &str,
    source: &AttributeContract<'db>,
    target: &AttributeContract<'db>,
) -> Option<AttributeViolation<'db>> {
    if source.is_method && target.is_method
        || target.qualifiers.contains(TypeQualifiers::FINAL)
        || !source.is_property
            && !target.is_property
            && !source.is_method
            && !target.is_method
            && source.qualifiers.contains(TypeQualifiers::CLASS_VAR)
                != target.qualifiers.contains(TypeQualifiers::CLASS_VAR)
    {
        return None;
    }
    if !source.read.is_assignable_to(db, env, target.read) {
        return Some(AttributeViolation::Read {
            source: source.read,
            target: target.read,
        });
    }
    let write = target.write?;
    // A neutral dataclass-transform base explicitly permits frozen subclasses. Its
    // fields can become read-only there, even though writes to the base are allowed.
    if !target.is_property
        && target_receiver
            .nominal_class(db, env)
            .and_then(|class| class.static_class_literal(db))
            .is_some_and(|(literal, _)| literal.is_neutral_dataclass(db))
        && receiver
            .nominal_class(db, env)
            .and_then(|class| class.static_class_literal(db))
            .is_some_and(|(literal, _)| {
                literal.is_frozen_dataclass(db) == Some(true)
                    && literal.is_own_dataclass_instance_field(db, name)
            })
    {
        return None;
    }
    let receiver = if target.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
        receiver.to_meta_type(db, env)
    } else {
        receiver
    };
    let target_receiver = if target.qualifiers.contains(TypeQualifiers::CLASS_VAR) {
        target_receiver.to_meta_type(db, env)
    } else {
        target_receiver
    };
    if source.write.is_none() || !receiver.is_attribute_writable_with(db, env, name, write) {
        // Do not require a write that the superclass itself cannot perform, for example
        // when a descriptor or custom `__setattr__` rejects the declared value type.
        if !target_receiver.is_attribute_writable_with(db, env, name, write) {
            return None;
        }
        return Some(if source.write.is_none() {
            AttributeViolation::ReadOnly
        } else {
            AttributeViolation::Write { target: write }
        });
    }
    None
}

/// Report an incompatible explicit override, if its diagnostic rule is enabled.
///
/// Return `true` only when a diagnostic was emitted, so callers can stop after the first
/// reported base conflict. Property incompatibilities and ordinary mutable narrowing
/// have separate rules.
pub(super) fn check_override<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    superclass: ClassType<'db>,
    inherited_owner: Option<ClassType<'db>>,
    name: &Name,
    definition: Definition<'db>,
    superclass_definition: Option<Definition<'db>>,
) -> bool {
    let db = context.db();
    let env = &context.program_environment();
    let receiver = Type::instance(db, env, class);
    let Some(source) = attribute_contract(db, env, class, receiver, name) else {
        return false;
    };
    let Some(target) = attribute_contract(db, env, superclass, receiver, name) else {
        return false;
    };
    let Some(violation) = attribute_violation(
        db,
        env,
        receiver,
        Type::instance(db, env, superclass),
        name,
        &source,
        &target,
    ) else {
        return false;
    };
    // Suppress only conflicts already present with the parent's own specialization of
    // this ancestor. An unrelated base, or a newly incompatible specialization in the
    // child, must still be checked.
    if let Some(parent) = inherited_owner
        && parent != superclass
    {
        let parent_receiver = Type::instance(db, env, parent);
        if let Some(parent_source) = attribute_contract(db, env, parent, parent_receiver, name)
            && parent
                .iter_mro(db)
                .skip(1)
                .filter_map(ClassBase::into_class)
                .chain(parent.iter_explicit_ancestors(db, env).skip(1))
                .filter(|ancestor| ancestor.class_literal(db) == superclass.class_literal(db))
                .any(|ancestor| {
                    attribute_contract(db, env, ancestor, parent_receiver, name).is_some_and(
                        |parent_target| {
                            attribute_violation(
                                db,
                                env,
                                parent_receiver,
                                Type::instance(db, env, ancestor),
                                name,
                                &parent_source,
                                &parent_target,
                            )
                            .is_some()
                        },
                    )
                })
        {
            return false;
        }
    }
    let involves_property = source.is_property || target.is_property;
    let rule = if involves_property {
        &INVALID_PROPERTY_TYPE_OVERRIDE
    } else if matches!(violation, AttributeViolation::Write { .. }) {
        &INVALID_MUTABLE_OVERRIDE
    } else {
        &INVALID_ATTRIBUTE_OVERRIDE
    };
    let Some(builder) = context.report_lint(rule, definition.focus_range(db, context.module()))
    else {
        return false;
    };
    let mut diagnostic =
        builder.into_diagnostic(format_args!("Invalid override of attribute `{name}`"));
    match violation {
        AttributeViolation::Read { source, target } => {
            if involves_property {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Read type `{}` is not assignable to inherited read type `{}`",
                    source.display(db, env),
                    target.display(db, env),
                ));
            } else {
                diagnostic.set_primary_annotation_message(format_args!(
                    "Type `{}` is not assignable to inherited type `{}`",
                    source.display(db, env),
                    target.display(db, env),
                ));
            }
        }
        AttributeViolation::Write { target } => {
            diagnostic.set_primary_annotation_message(format_args!(
                "Override does not accept writes of type `{}`",
                target.display(db, env),
            ));
        }
        AttributeViolation::ReadOnly => diagnostic
            .set_primary_annotation_message("Read-only attribute overrides a writable attribute"),
    }
    if let Some(base_definition) = superclass_definition
        && base_definition.file(db) == context.file()
    {
        diagnostic.annotate(
            Annotation::secondary(context.span(base_definition.focus_range(db, context.module())))
                .message(format_args!(
                    "`{}.{name}` declared here",
                    superclass.name(db)
                )),
        );
    }
    true
}
