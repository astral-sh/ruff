//! Compatibility of the values read from and written to inherited attributes.

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use rustc_hash::FxHashSet;
use ty_python_core::{definition::Definition, place_table};

use crate::{
    Db, ProgramEnvironment,
    place::{Place, TypeOrigin},
    types::{
        ClassBase, ClassType, IntersectionType, MemberLookupPolicy, StaticClassLiteral, Type,
        TypeQualifiers,
        attribute_write::{DescriptorSetterDomain, descriptor_setter_domain},
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_ATTRIBUTE_OVERRIDE, INVALID_MUTABLE_OVERRIDE, INVALID_PROPERTY_TYPE_OVERRIDE,
        },
        list_members::{MemberWithDefinition, all_end_of_scope_members},
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
    qualifiers: TypeQualifiers,
}

/// Resolve an owner's declaration as seen through the receiver being checked.
///
/// Keep the owner's generic specialization, but bind `Self` and descriptor access to
/// `receiver`. Looking up the name directly on the receiver would hide an overridden
/// declaration before its contract could be compared. Methods and fields handled by
/// dedicated override rules return `None`.
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
    // `object.__class__` is specialized by member lookup, including for protocols
    // that describe exact runtime classes. Its synthetic `Self` is not an override contract.
    if owner.is_object(db) && name == "__class__" {
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
    let own_place = match (class_member.place, instance_member.place) {
        (Place::Defined(place), _) | (_, Place::Defined(place)) => place,
        (Place::Undefined, Place::Undefined) => return None,
    };
    // Method contracts have their own override checks, including conditional definitions.
    // A descriptor decorator, however, can turn a function into an attribute.
    let is_method = |ty| {
        matches!(ty, Type::FunctionLiteral(_))
            || matches!(ty, Type::Callable(callable) if callable.is_method_like(db))
    };
    if match own_place.ty {
        Type::Union(union) => union.elements(db).iter().copied().all(is_method),
        ty => is_method(ty) || matches!(ty, Type::TypeAlias(_)),
    } {
        return None;
    }
    let qualifiers = class_member.qualifiers | instance_member.qualifiers;
    let is_final = qualifiers.contains(TypeQualifiers::FINAL);
    let is_class_var = qualifiers.contains(TypeQualifiers::CLASS_VAR);
    let is_property = own_place.ty.as_property_instance().is_some();
    let is_descriptor = !is_class_var
        && !matches!(own_place.ty, Type::SlotDescriptor(_))
        && !class_member.place.is_undefined()
        && own_place
            .ty
            .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
            .place
            .ignore_possibly_undefined()
            .is_some();
    // TODO: Check ordinary unannotated initializers once inherited type context is
    // available in ordinary inference. Raw binding types can retain literals that
    // attribute access widens; they do not define an independent write contract.
    if own_place.origin == TypeOrigin::Inferred && !is_descriptor {
        return None;
    }
    let (read, write) = if is_descriptor {
        let read = own_place
            .ty
            .try_call_dunder_get(db, env, Some(receiver), receiver.to_meta_type(db, env))
            .ok()??
            .return_type;
        let write = descriptor_write_domain(db, env, own_place.ty, receiver, read);
        (read, write)
    } else {
        let place = if (is_class_var || is_final) && !class_member.place.is_undefined() {
            class_member.place
        } else {
            instance_member.place
        };
        let read = place
            .ignore_possibly_undefined()?
            .bind_self_typevars(db, env, receiver);
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
    if target.qualifiers.contains(TypeQualifiers::FINAL)
        || source.qualifiers.contains(TypeQualifiers::CLASS_VAR)
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
    if !source
        .write
        .is_some_and(|source_write| write.is_assignable_to(db, env, source_write))
        || !receiver.is_attribute_writable_with(db, env, name, write)
    {
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
    if already_inherited(db, env, class, superclass, name, &source) {
        return false;
    }

    let rule = if source.is_property || target.is_property {
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
            diagnostic.set_primary_annotation_message(format_args!(
                "Type `{}` is not assignable to inherited type `{}`",
                source.display(db, env),
                target.display(db, env),
            ));
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

/// Check receiver annotations that introduce declarations outside the class body.
///
/// Only explicit receiver annotations introduce a new contract; ordinary assignments
/// use the inherited one. Check base names before inferring the receiver declarations
/// to avoid evaluating unrelated method bodies. Names also declared in the class body
/// are handled by `check_override` through the class-member pass.
///
/// ```python
/// class Base:
///     value: int
///
/// class Child(Base):
///     def __init__(self) -> None:
///         self.value: str = ""  # Declares an incompatible override of Base.value.
/// ```
pub(super) fn check_instance_overrides<'db>(
    context: &InferContext<'db, '_>,
    class: ClassType<'db>,
    bases: &[ClassBase<'db>],
) {
    let db = context.db();
    let env = &context.program_environment();
    for name in class.own_instance_attribute_names(db) {
        if is_mangled_private(name) || !class.own_class_member(db, env, None, name).is_undefined() {
            continue;
        }
        // Avoid inferring method bodies for names that do not override anything.
        if !bases
            .iter()
            .filter_map(|base| base.into_class())
            .any(|base| {
                !base.own_instance_member(db, env, name).is_undefined()
                    || !base.own_class_member(db, env, None, name).is_undefined()
            })
        {
            continue;
        }
        let Place::Defined(place) = class.own_instance_member(db, env, name).inner.place else {
            continue;
        };
        if place.origin != TypeOrigin::Declared {
            continue;
        }
        let Some(definition) = place.provenance.definition() else {
            continue;
        };
        for base in bases.iter().filter_map(|base| base.into_class()) {
            let base_member = base.own_instance_member(db, env, name).inner;
            let base_definition = match base_member.place {
                Place::Defined(place) => place.provenance.definition(),
                Place::Undefined => None,
            };
            if check_override(context, class, base, name, definition, base_definition) {
                break;
            }
        }
    }
}

/// Whether a parent already contains the same incompatible attribute contract.
///
/// Inspect every parent hierarchy, since the conflicting parent need not be first in
/// the child's MRO. Match the selected read/write contract in the child's context, then
/// recheck the conflict with the parent's own specializations. A parent that satisfies
/// `Base[Any]` cannot suppress a conflict with a newly inherited `Base[int]`.
fn already_inherited<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    target_owner: ClassType<'db>,
    name: &str,
    source: &AttributeContract<'db>,
) -> bool {
    let child_receiver = Type::instance(db, env, class);
    class
        .iter_mro(db)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .any(|parent| {
            let Some(owner) = parent
                .iter_mro(db)
                .filter_map(ClassBase::into_class)
                .find(|owner| {
                    !owner.own_class_member(db, env, None, name).is_undefined()
                        || !owner.own_instance_member(db, env, name).is_undefined()
                })
            else {
                return false;
            };
            let Some(inherited) = attribute_contract(db, env, owner, child_receiver, name) else {
                return false;
            };
            if inherited.read != source.read
                || inherited.write != source.write
                || inherited.qualifiers != source.qualifiers
            {
                return false;
            }
            let receiver = Type::instance(db, env, parent);
            let Some(inherited) = attribute_contract(db, env, owner, receiver, name) else {
                return false;
            };
            parent
                .iter_mro(db)
                .skip(1)
                .filter_map(ClassBase::into_class)
                .chain(parent.iter_explicit_ancestors(db, env).skip(1))
                .filter(|ancestor| ancestor.class_literal(db) == target_owner.class_literal(db))
                .any(|ancestor| {
                    let Some(target) = attribute_contract(db, env, ancestor, receiver, name) else {
                        return false;
                    };
                    attribute_violation(
                        db,
                        env,
                        receiver,
                        Type::instance(db, env, ancestor),
                        name,
                        &inherited,
                        &target,
                    )
                    .is_some()
                })
        })
}

/// Check the attribute selected by an MRO against every inherited declaration of that name.
///
/// Explicit overrides are checked separately. Lookup selects only one source before the
/// first dynamic base, but all inherited generic specializations remain target contracts.
/// Conflicts already present in a parent hierarchy are suppressed.
///
/// ```python
/// class Integer:
///     value: int
///
/// class String:
///     value: str
///
/// class Combined(Integer, String): ...  # Integer.value cannot satisfy String.value.
/// ```
pub(super) fn check_inherited_conflicts<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_type: ClassType<'db>,
    own_members: &FxHashSet<MemberWithDefinition<'db>>,
) {
    let Some((mro, first_dynamic_base)) = super::inherited_conflict_mro(context, class, class_type)
    else {
        return;
    };
    let db = context.db();
    let env = &context.program_environment();
    let receiver = Type::instance(db, env, class_type);
    let mut seen: FxHashSet<Name> = own_members
        .iter()
        .map(|member| member.member.name.clone())
        .collect();
    seen.extend(
        class_type
            .own_instance_attribute_names(db)
            .iter()
            .filter(|name| {
                matches!(class_type.own_instance_member(db, env, name).inner.place,
            Place::Defined(place) if place.origin == TypeOrigin::Declared)
            })
            .cloned(),
    );
    let contracts: Vec<_> = mro
        .iter()
        .copied()
        .chain(class_type.iter_explicit_ancestors(db, env).skip(1))
        .collect();
    for (index, owner) in mro.iter().copied().enumerate() {
        if first_dynamic_base.is_some_and(|position| index >= position) {
            break;
        }
        let Some((literal, _)) = owner.static_class_literal(db) else {
            continue;
        };
        let names = all_end_of_scope_members(db, literal.body_scope(db))
            .map(|member| member.member.name)
            .chain(owner.own_instance_attribute_names(db).iter().cloned());
        for name in names {
            if is_mangled_private(&name) || !seen.insert(name.clone()) {
                continue;
            }
            let Some(source) = attribute_contract(db, env, owner, receiver, &name) else {
                continue;
            };
            for target_owner in contracts.iter().copied().filter(|target| *target != owner) {
                let Some(target) = attribute_contract(db, env, target_owner, receiver, &name)
                else {
                    continue;
                };
                let Some(violation) = attribute_violation(
                    db,
                    env,
                    receiver,
                    Type::instance(db, env, target_owner),
                    &name,
                    &source,
                    &target,
                ) else {
                    continue;
                };
                if already_inherited(db, env, class_type, target_owner, &name, &source) {
                    continue;
                }
                let rule = if source.is_property || target.is_property {
                    &INVALID_PROPERTY_TYPE_OVERRIDE
                } else if matches!(violation, AttributeViolation::Write { .. }) {
                    &INVALID_MUTABLE_OVERRIDE
                } else {
                    &INVALID_ATTRIBUTE_OVERRIDE
                };
                let Some(builder) = context.report_lint(rule, class.header_range(db)) else {
                    continue;
                };
                let mut diagnostic = builder
                    .into_diagnostic(format_args!("Incompatible inherited attribute `{name}`"));
                diagnostic.set_primary_annotation_message(format_args!(
                    "`{}.{name}` is incompatible with `{}.{name}`",
                    owner.name(db),
                    target_owner.name(db),
                ));
                match violation {
                    AttributeViolation::Read { source, target } => diagnostic.info(format_args!(
                        "Type `{}` is not assignable to inherited type `{}`",
                        source.display(db, env),
                        target.display(db, env),
                    )),
                    AttributeViolation::Write { target } => diagnostic.info(format_args!(
                        "Inherited attribute does not accept writes of type `{}`",
                        target.display(db, env),
                    )),
                    AttributeViolation::ReadOnly => diagnostic
                        .info("Inherited read-only attribute replaces a writable attribute"),
                }
                for owner in [owner, target_owner] {
                    let Some((literal, _)) = owner.static_class_literal(db) else {
                        continue;
                    };
                    let definition = place_table(db, literal.body_scope(db))
                        .symbol_id(&name)
                        .and_then(|id| super::symbol_definition(db, literal.body_scope(db), id))
                        .or_else(
                            || match owner.own_instance_member(db, env, &name).inner.place {
                                Place::Defined(place) => place.provenance.definition(),
                                Place::Undefined => None,
                            },
                        );
                    if let Some(definition) = definition
                        && definition.file(db) == context.file()
                    {
                        diagnostic.annotate(
                            Annotation::secondary(
                                context.span(definition.focus_range(db, context.module())),
                            )
                            .message(format_args!("`{}.{name}` declared here", owner.name(db))),
                        );
                    }
                }
                break;
            }
        }
    }
}
