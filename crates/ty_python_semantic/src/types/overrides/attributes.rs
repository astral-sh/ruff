//! Compatibility of the values read from and written to inherited attributes.

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use ty_python_core::{definition::Definition, place_table};

use crate::{
    Db, ProgramEnvironment,
    lint::LintMetadata,
    place::{Place, TypeOrigin},
    types::{
        ClassBase, ClassType, InstanceFallbackShadowsNonDataDescriptor, IntersectionType,
        KnownInstanceType, MemberLookupPolicy, StaticClassLiteral, Type, TypeQualifiers,
        attribute_write::{DescriptorSetterDomain, descriptor_setter_domain},
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_ATTRIBUTE_OVERRIDE, INVALID_MUTABLE_OVERRIDE, INVALID_PROPERTY_TYPE_OVERRIDE,
        },
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttributeKind {
    Value,
    Method,
    Property,
}

/// The instance operations promised by one attribute declaration.
///
/// A descriptor can accept a different type from the one it returns. Ordinary mutable
/// attributes use the same type for both operations, making their types invariant.
struct AttributeContract<'db> {
    /// An unannotated assignment in this class is checked as an assignment, not
    /// a new read-type declaration. Inherited values still have readable types.
    read: Option<Type<'db>>,
    write: Option<Type<'db>>,
    /// Whether this member establishes read/write type constraints for overrides.
    ///
    /// Declared types (including inherited annotations) and descriptors establish such
    /// constraints. An inferred default does not: its value can still be checked against
    /// a base's declared read type, but its narrow inferred type does not restrict later
    /// writes. A descriptor's getter and setter provide the constraints instead of an
    /// attribute annotation. A read-only descriptor can have this flag set while
    /// `write` is `None`.
    ///
    /// ```python
    /// class Declared:
    ///     value: int = 0   # has_type_contract = true
    ///
    /// class Same(Declared):
    ///     value = 1        # true: retains the inherited int annotation
    ///
    /// class Number:
    ///     value = 1        # false: only an inferred default
    ///
    /// class Text:
    ///     value = "text"   # false: only an inferred default
    ///
    /// class Compatible(Number, Declared): ...  # 1 is an int; later int writes are fine
    /// class Incompatible(Text, Declared): ...  # error: "text" is not an int
    /// ```
    ///
    /// In `Compatible`, `Number.value` has a `read` type but no type contract. When
    /// checking it as an override, compare that read with `Declared.value`; do not treat
    /// `Literal[1]` as the only permitted write. When a member without a type contract
    /// is the target, it imposes no declared read/write type on the override. Storage
    /// (for example, `ClassVar` versus instance storage) is checked independently.
    has_type_contract: bool,
    kind: AttributeKind,
    is_frozen_field: bool,
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
    let inherit_instance_contract = class_member.place.is_undefined()
        && matches!(instance_member.place, Place::Defined(place) if place.origin == TypeOrigin::Inferred);
    let class_member = if inherit_instance_contract {
        // An unannotated `self.x = ...` retains a class-body annotation or descriptor
        // inherited by this owner. Looking on the receiver instead could take a contract
        // from an unrelated base and hide an actual inherited conflict.
        owner.class_member(db, env, name, MemberLookupPolicy::default())
    } else {
        class_member
    };
    let is_slot = matches!(
        class_member.place.ignore_possibly_undefined(),
        Some(Type::SlotDescriptor(_))
    );
    let instance_member = if is_slot || inherit_instance_contract {
        // A slot provides storage without replacing an inherited annotation:
        //
        // ```python
        // class Base:
        //     value: int
        //
        // class Slotted(Base):
        //     __slots__ = ("value",)
        //
        //     def set_value(self, value):
        //         self.value = value
        //
        // class Child(Slotted):
        //     value: str  # Incompatible with Base.value.
        // ```
        //
        // For `owner = Slotted`, looking only at its own assignments would infer `Unknown`
        // for `value`. The same applies to other unannotated instance assignments. Full lookup
        // on the owner preserves annotations inherited by that owner. Looking on the receiver
        // instead would pick up declarations from unrelated bases and hide real conflicts.
        owner.instance_member(db, env, name)
    } else {
        instance_member
    };
    let qualifiers = class_member.qualifiers | instance_member.qualifiers;
    let is_final = qualifiers.contains(TypeQualifiers::FINAL);
    let is_class_var = qualifiers.contains(TypeQualifiers::CLASS_VAR);
    let is_descriptor = !is_class_var
        && class_member
            .place
            .ignore_possibly_undefined()
            .is_some_and(|ty| {
                is_slot
                    || ty
                        .class_member_with_policy(
                            db,
                            env,
                            "__get__",
                            MemberLookupPolicy::REQUIRE_CONCRETE,
                        )
                        .place
                        .ignore_possibly_undefined()
                        .is_some()
            });
    let own_place = match (class_member.place, instance_member.place) {
        // An inherited, unannotated class default does not constrain values stored
        // on an instance. Keep the inferred assignment when no descriptor governs it.
        (Place::Defined(class), Place::Defined(instance))
            if inherit_instance_contract && !class.origin.is_declared() && !is_descriptor =>
        {
            instance
        }
        (Place::Defined(place), _) | (_, Place::Defined(place)) => place,
        (Place::Undefined, Place::Undefined) => return None,
    };
    if matches!(own_place.ty, Type::TypeAlias(_)) {
        return None;
    }
    let alternatives = own_place
        .ty
        .as_union()
        .map_or(std::slice::from_ref(&own_place.ty), |union| {
            union.elements(db)
        });
    // Only pairs of methods go to the method checker. A decorator can turn a
    // function into a property, in which case its exposed value must be checked.
    let kind = if alternatives.iter().all(|ty| {
        matches!(ty, Type::FunctionLiteral(_))
            || matches!(ty, Type::Callable(callable) if callable.is_method_like(db))
    }) {
        AttributeKind::Method
    } else if alternatives.iter().any(Type::is_property_instance) {
        AttributeKind::Property
    } else {
        AttributeKind::Value
    };
    let is_frozen_field = literal.is_frozen_dataclass(db) == Some(true)
        && literal.is_own_dataclass_instance_field(db, name);
    // Defaults with inherited annotations already have a declared type. Other inferred
    // bindings still determine storage, but their raw types can retain literals that
    // ordinary attribute access widens.
    let has_type_contract = own_place.origin == TypeOrigin::Declared || is_descriptor;
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
        let read = if kind == AttributeKind::Method
            || matches!(
                own_place.ty,
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
            descriptor_write_domain(db, env, own_place.ty, receiver, read)
        };
        (read, write)
    } else {
        // The selected declaration defines the contract, including inherited annotations.
        // Inferred instance assignments do not narrow a class-body declaration.
        let read = own_place.ty.bind_self_typevars(db, env, receiver);
        (
            read,
            Some(
                owner
                    .converter_input_type_for_field(db, name)
                    .unwrap_or(read),
            ),
        )
    };
    let check_read = has_type_contract
        || receiver
            .nominal_class(db, env)
            .is_some_and(|receiver_class| receiver_class != owner);
    Some(AttributeContract {
        read: check_read.then_some(read),
        write: if is_final || !is_class_var && literal.is_frozen_dataclass(db) == Some(true) {
            None
        } else {
            write
        },
        has_type_contract,
        kind,
        is_frozen_field,
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

impl AttributeViolation<'_> {
    /// Property violations have a dedicated rule, including incompatible writes.
    const fn rule(&self, involves_property: bool) -> &'static LintMetadata {
        match self {
            _ if involves_property => &INVALID_PROPERTY_TYPE_OVERRIDE,
            Self::Write { .. } => &INVALID_MUTABLE_OVERRIDE,
            Self::Read { .. } | Self::ReadOnly => &INVALID_ATTRIBUTE_OVERRIDE,
        }
    }
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
    if source.kind == AttributeKind::Method && target.kind == AttributeKind::Method
        || target.qualifiers.contains(TypeQualifiers::FINAL)
        || source.kind == AttributeKind::Value
            && target.kind == AttributeKind::Value
            && source.qualifiers.contains(TypeQualifiers::CLASS_VAR)
                != target.qualifiers.contains(TypeQualifiers::CLASS_VAR)
    {
        return None;
    }
    if !target.has_type_contract {
        return None;
    }
    let (source_read, target_read) = (source.read?, target.read?);
    if !source_read.is_assignable_to(db, env, target_read) {
        return Some(AttributeViolation::Read {
            source: source_read,
            target: target_read,
        });
    }
    // Inferring a narrow default does not declare that subsequent writes must have
    // that same narrow type. Only annotated attributes and descriptors constrain them.
    if !source.has_type_contract {
        return None;
    }
    let write = target.write?;
    // A neutral dataclass-transform base explicitly permits frozen subclasses. Its
    // fields can become read-only there, even though writes to the base are allowed.
    // Preserve this permission when that frozen field is inherited by another subclass.
    if target.kind != AttributeKind::Property
        && target_receiver
            .nominal_class(db, env)
            .and_then(|class| class.static_class_literal(db))
            .is_some_and(|(literal, _)| literal.is_neutral_dataclass(db))
        && source.is_frozen_field
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
    if already_inherited(db, env, class, superclass, name, &source) {
        return false;
    }
    let involves_property =
        source.kind == AttributeKind::Property || target.kind == AttributeKind::Property;
    let rule = violation.rule(involves_property);
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
    let Some((literal, _)) = class.static_class_literal(db) else {
        return;
    };
    for name in class.own_instance_attribute_names(db) {
        // Ordinary assignments introduce no override contract. Do not infer their method
        // bodies just to discard the inferred type; a lazy cache can depend on its own reads.
        if is_mangled_private(name)
            || !super::has_own_instance_declaration(db, literal.body_scope(db), name)
            || !class.own_class_member(db, env, None, name).is_undefined()
        {
            continue;
        }
        // Avoid inferring method bodies for names that do not override anything.
        let Some(inherited_owner) =
            bases
                .iter()
                .filter_map(|base| base.into_class())
                .find(|base| {
                    !base.own_instance_member(db, env, name).is_undefined()
                        || !base.own_class_member(db, env, None, name).is_undefined()
                })
        else {
            continue;
        };
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
            if check_override(
                context,
                class,
                base,
                Some(inherited_owner),
                name,
                definition,
                base_definition,
            ) {
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
                || inherited.has_type_contract != source.has_type_contract
                || inherited.is_frozen_field != source.is_frozen_field
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

/// Check the selected attribute against every inherited declaration of that name.
///
/// Explicit overrides are checked separately. All inherited generic specializations remain
/// target contracts, and conflicts already present in a parent hierarchy are suppressed.
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
pub(super) fn check_inherited_conflict<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    class_type: ClassType<'db>,
    owner: ClassType<'db>,
    contracts: &[ClassType<'db>],
    name: &Name,
) {
    let db = context.db();
    let env = &context.program_environment();
    let receiver = Type::instance(db, env, class_type);
    let Some(source) = attribute_contract(db, env, owner, receiver, name) else {
        return;
    };
    for target_owner in contracts.iter().copied().filter(|target| *target != owner) {
        let Some(target) = attribute_contract(db, env, target_owner, receiver, name) else {
            continue;
        };
        let Some(violation) = attribute_violation(
            db,
            env,
            receiver,
            Type::instance(db, env, target_owner),
            name,
            &source,
            &target,
        ) else {
            continue;
        };
        if already_inherited(db, env, class_type, target_owner, name, &source) {
            continue;
        }
        let rule = violation
            .rule(source.kind == AttributeKind::Property || target.kind == AttributeKind::Property);
        let Some(builder) = context.report_lint(rule, class.header_range(db)) else {
            continue;
        };
        let mut diagnostic =
            builder.into_diagnostic(format_args!("Incompatible inherited attribute `{name}`"));
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
            AttributeViolation::ReadOnly => {
                diagnostic.info("Inherited read-only attribute replaces a writable attribute");
            }
        }
        for owner in [owner, target_owner] {
            let Some((literal, _)) = owner.static_class_literal(db) else {
                continue;
            };
            let definition = place_table(db, literal.body_scope(db))
                .symbol_id(name)
                .and_then(|id| super::symbol_definition(db, literal.body_scope(db), id))
                .or_else(
                    || match owner.own_instance_member(db, env, name).inner.place {
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
