//! Compatibility of the values read from and written to inherited attributes.

use ruff_db::diagnostic::Annotation;
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_mangled_private;
use ty_python_core::definition::{Definition, DefinitionKind};

use crate::{
    Db, ProgramEnvironment, attribute_declarations,
    lint::LintMetadata,
    place::{DefinedPlace, Place, TypeOrigin},
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

/// An inferred default has no declared attribute contract, but its value can still
/// provide a call signature that an overriding method needs to preserve.
enum MemberContract<'db> {
    Attribute(AttributeContract<'db>),
    Inferred(Type<'db>),
}

/// Selects the declaration that supplies an attribute's override contract.
///
/// A class default can retain an inherited annotation while a receiver annotation
/// introduces a different type. Check each definition separately, then preserve the
/// receiver declaration when checking explicit overrides in further subclasses:
///
/// ```python
/// class Base:
///     value: int
///
/// class Parent(Base):
///     value = 0
///
///     def __init__(self) -> None:
///         self.value: str = ""
///
/// class Child(Parent):
///     value: str
/// ```
///
/// `ClassBody` uses the inherited `int` contract for `Parent.value`. `Receiver`
/// checks its explicit `str` annotation and reports the incompatible override.
/// `Inherited` retains that `str` declaration when checking `Child`, so repeating
/// the annotation does not report the parent's conflict again.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttributeContractKind {
    /// Prefer the class member, including inherited annotations on unannotated defaults.
    ClassBody,
    /// Check a receiver annotation independently of any class default or descriptor.
    Receiver,
    /// Prefer a receiver declaration over an ordinary default, preserving descriptor and
    /// `ClassVar` contracts.
    Inherited,
}

/// Whether every alternative has method binding behavior rather than only a call signature.
fn is_method_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    ty.as_union()
        .map_or(std::slice::from_ref(&ty), |union| union.elements(db))
        .iter()
        .all(|ty| {
            matches!(ty, Type::FunctionLiteral(_))
                || matches!(ty, Type::Callable(callable) if callable.is_method_like(db))
        })
}

/// Resolve an owner's declaration as seen through the receiver being checked.
///
/// Keep the owner's generic specialization, but bind `Self` and descriptor access to
/// `receiver`. Looking up the name directly on the receiver would hide an overridden
/// declaration before its contract could be compared. Fields handled by dedicated
/// override rules return `None`.
///
/// `kind` selects the declaration being checked. For inherited contracts, an explicit
/// receiver declaration takes precedence over an ordinary class default, while
/// descriptors retain their access semantics.
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
fn member_contract<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    owner: ClassType<'db>,
    receiver: Type<'db>,
    name: &str,
    kind: AttributeContractKind,
) -> Option<MemberContract<'db>> {
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
    let mut class_member = owner.own_class_member(db, env, None, name).inner;
    let instance_member = if kind != AttributeContractKind::Receiver
        && matches!(
            class_member.place.ignore_possibly_undefined(),
            Some(Type::SlotDescriptor(_))
        ) {
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
        // for `value`. Full instance lookup preserves the inherited `int` annotation.
        owner.instance_member(db, env, name)
    } else {
        owner.own_instance_member(db, env, name).inner
    };
    let has_receiver_declaration = matches!(
        instance_member.place,
        Place::Defined(DefinedPlace { origin: TypeOrigin::Declared, provenance, .. })
            if provenance.definition().is_some_and(|definition| definition.place(db).is_member())
    );
    if kind == AttributeContractKind::Receiver
        || kind == AttributeContractKind::Inherited
            && has_receiver_declaration
            && !class_member.qualifiers.contains(TypeQualifiers::CLASS_VAR)
            && class_member
                .place
                .ignore_possibly_undefined()
                .is_none_or(|ty| {
                    ty.class_member_with_policy(
                        db,
                        env,
                        "__get__",
                        MemberLookupPolicy::REQUIRE_CONCRETE,
                    )
                    .place
                    .is_undefined()
                })
    {
        // An unannotated default may inherit an ancestor's type. It must not hide the
        // receiver declaration when this class is compared with its own subclasses.
        class_member = Place::Undefined.into();
    }
    let own_place = match (class_member.place, instance_member.place) {
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
    // A decorator can turn a function into a property, in which case its exposed
    // value must be checked instead of the original function's signature.
    let is_method = is_method_type(db, own_place.ty);
    let qualifiers = class_member.qualifiers | instance_member.qualifiers;
    let is_final = qualifiers.contains(TypeQualifiers::FINAL);
    let is_class_var = qualifiers.contains(TypeQualifiers::CLASS_VAR);
    let is_property = alternatives.iter().any(Type::is_property_instance);
    let is_slot = matches!(own_place.ty, Type::SlotDescriptor(_));
    let is_descriptor = !is_class_var
        && !class_member.place.is_undefined()
        && (is_slot
            || own_place
                .ty
                .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
                .place
                .ignore_possibly_undefined()
                .is_some());
    // Unannotated defaults with an inherited annotation already have a declared type.
    // Other inferred bindings do not define an independent write contract: their raw
    // types can retain literals that ordinary attribute access widens.
    if own_place.origin == TypeOrigin::Inferred && !is_descriptor {
        return Some(MemberContract::Inferred(own_place.ty));
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
    Some(MemberContract::Attribute(AttributeContract {
        read,
        // Attributes on `object` are included here because typeshed declares attributes such as
        // `__module__: str` on `object`, which normally implies writable instance attributes. But
        // `object()` provides no instance-attribute storage, so it does not actually guarantee
        // writability of these attributes.
        write: if owner.is_object(db)
            || is_final
            || !is_class_var && literal.is_frozen_dataclass(db) == Some(true)
        {
            None
        } else {
            write
        },
        is_property,
        is_method,
        qualifiers,
    }))
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

/// Select the checker from the members' access semantics before applying diagnostic settings.
pub(super) enum OverrideCheck<'db> {
    /// Compare an overriding method with an inherited callable's signature.
    Callable,
    /// Attribute checking is complete. `None` includes compatible pairs and conflicts
    /// already present in a base class.
    Attribute(Option<AttributeOverrideViolation<'db>>),
    /// At least one member has no attribute contract that we can check.
    Unavailable,
}

pub(super) struct AttributeOverrideViolation<'db> {
    violation: AttributeViolation<'db>,
    involves_property: bool,
}

impl<'db> AttributeOverrideViolation<'db> {
    pub(super) fn rule(&self) -> &'static LintMetadata {
        if self.involves_property {
            &INVALID_PROPERTY_TYPE_OVERRIDE
        } else if matches!(self.violation, AttributeViolation::Write { .. }) {
            &INVALID_MUTABLE_OVERRIDE
        } else {
            &INVALID_ATTRIBUTE_OVERRIDE
        }
    }

    pub(super) fn report(
        &self,
        context: &InferContext<'db, '_>,
        superclass: ClassType<'db>,
        name: &Name,
        definition: Definition<'db>,
        superclass_definition: Option<Definition<'db>>,
    ) {
        let db = context.db();
        let env = &context.program_environment();
        let rule = self.rule();
        let Some(builder) = context.report_lint(rule, definition.focus_range(db, context.module()))
        else {
            return;
        };
        let mut diagnostic =
            builder.into_diagnostic(format_args!("Invalid override of attribute `{name}`"));
        match self.violation {
            AttributeViolation::Read { source, target } => {
                if self.involves_property {
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
            AttributeViolation::ReadOnly => diagnostic.set_primary_annotation_message(
                "Read-only attribute overrides a writable attribute",
            ),
        }
        if let Some(base_definition) = superclass_definition
            && base_definition.file(db) == context.file()
        {
            diagnostic.annotate(
                Annotation::secondary(
                    context.span(base_definition.focus_range(db, context.module())),
                )
                .message(format_args!(
                    "`{}.{name}` declared here",
                    superclass.name(db)
                )),
            );
        }
    }
}

/// Classify an override and check attribute compatibility without emitting diagnostics.
///
/// Declared attributes compare the values exposed after descriptor access. An
/// inferred callable default has no declared attribute contract, but an overriding
/// method must still preserve its call signature. Methods replacing final callable
/// attributes also retain signature checking, independently of finality diagnostics.
/// Diagnostic settings do not affect which check applies.
pub(super) fn check_override<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    superclass: ClassType<'db>,
    inherited_owner: Option<ClassType<'db>>,
    name: &Name,
    definition: Definition<'db>,
) -> OverrideCheck<'db> {
    // Classify methods before constructing attribute contracts: some members, such as
    // `object.__hash__`, have method signatures but no general attribute contract.
    // Receiver annotations still describe instance storage even if a method with
    // the same name exists in the class body.
    if definition.place(db).is_symbol()
        && [class, superclass].into_iter().all(|owner| {
            owner
                .own_class_member(db, env, None, name)
                .inner
                .place
                .ignore_possibly_undefined()
                .is_some_and(|ty| is_method_type(db, ty))
        })
    {
        return OverrideCheck::Callable;
    }
    let receiver = Type::instance(db, env, class);
    let source_kind = if definition.place(db).is_member() {
        AttributeContractKind::Receiver
    } else {
        AttributeContractKind::ClassBody
    };
    // An unannotated class default retains its inherited class-body annotation. Compare
    // it with the same contracts on the bases, rather than rechecking receiver declarations.
    let inherited_kind = if matches!(
        definition.kind(db),
        DefinitionKind::Assignment(_) | DefinitionKind::AugmentedAssignment(_)
    ) {
        AttributeContractKind::ClassBody
    } else {
        AttributeContractKind::Inherited
    };
    let Some(MemberContract::Attribute(source)) =
        member_contract(db, env, class, receiver, name, source_kind)
    else {
        return OverrideCheck::Unavailable;
    };
    let target = match member_contract(db, env, superclass, receiver, name, inherited_kind) {
        Some(MemberContract::Attribute(target)) => target,
        Some(MemberContract::Inferred(target))
            if source.is_method && target.try_upcast_to_callable(db, env).is_some() =>
        {
            return OverrideCheck::Callable;
        }
        Some(MemberContract::Inferred(_)) | None => return OverrideCheck::Unavailable,
    };
    // Final attributes are exempt from attribute compatibility checks, but finality
    // and signature compatibility are independent: suppressing a finality diagnostic
    // must not hide an incompatible method signature.
    if source.is_method
        && target.qualifiers.contains(TypeQualifiers::FINAL)
        && target.read.try_upcast_to_callable(db, env).is_some()
    {
        return OverrideCheck::Callable;
    }
    let Some(violation) = attribute_violation(
        db,
        env,
        receiver,
        Type::instance(db, env, superclass),
        name,
        &source,
        &target,
    ) else {
        return OverrideCheck::Attribute(None);
    };
    // Suppress only conflicts already present with the parent's own specialization of
    // this ancestor. An unrelated base, or a newly incompatible specialization in the
    // child, must still be checked.
    if let Some(parent) = inherited_owner
        && parent != superclass
    {
        let parent_receiver = Type::instance(db, env, parent);
        if let Some(MemberContract::Attribute(parent_source)) =
            member_contract(db, env, parent, parent_receiver, name, inherited_kind)
            && parent
                .iter_mro(db)
                .skip(1)
                .filter_map(ClassBase::into_class)
                .chain(parent.iter_explicit_ancestors(db, env).skip(1))
                .filter(|ancestor| ancestor.class_literal(db) == superclass.class_literal(db))
                .any(|ancestor| {
                    member_contract(db, env, ancestor, parent_receiver, name, inherited_kind)
                        .is_some_and(|parent_target| {
                            let MemberContract::Attribute(parent_target) = parent_target else {
                                return false;
                            };
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
                        })
                })
        {
            return OverrideCheck::Attribute(None);
        }
    }
    OverrideCheck::Attribute(Some(AttributeOverrideViolation {
        violation,
        involves_property: source.is_property || target.is_property,
    }))
}

/// Check receiver annotations that introduce declarations outside the class body.
///
/// Only explicit receiver annotations introduce a new contract; ordinary assignments
/// use the inherited one. Check base names before inferring the receiver declarations
/// to avoid evaluating unrelated method bodies. Class-body declarations are handled
/// by the class-member pass, but slots and unannotated defaults do not replace a
/// receiver declaration.
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
            || !attribute_declarations(db, literal.body_scope(db), name).any(
                |(mut declarations, _)| {
                    declarations.any(|declaration| declaration.declaration.definition().is_some())
                },
            )
        {
            continue;
        }
        // Avoid inferring method bodies for names that do not override anything. Ordinary
        // assignments in a base do not replace the inherited declaration's owner.
        let Some(inherited_owner) =
            bases
                .iter()
                .filter_map(|base| base.into_class())
                .find(|base| {
                    !base.own_class_member(db, env, None, name).is_undefined()
                        || matches!(
                            base.own_instance_member(db, env, name).inner.place,
                            Place::Defined(DefinedPlace {
                                origin: TypeOrigin::Declared,
                                ..
                            })
                        )
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
        if definition.place(db).is_symbol() {
            // `own_instance_member` can return a class-body declaration, which the
            // class-member pass has already checked.
            continue;
        }
        for base in bases.iter().filter_map(|base| base.into_class()) {
            let base_member = base.own_instance_member(db, env, name).inner;
            let base_definition = match base_member.place {
                Place::Defined(place) => place.provenance.definition(),
                Place::Undefined => None,
            }
            .or_else(
                || match base.own_class_member(db, env, None, name).inner.place {
                    Place::Defined(place) => place.provenance.definition(),
                    Place::Undefined => None,
                },
            );
            if let OverrideCheck::Attribute(Some(violation)) = check_override(
                db,
                env,
                class,
                base,
                Some(inherited_owner),
                name,
                definition,
            ) {
                violation.report(context, base, name, definition, base_definition);
                if context.is_lint_enabled(violation.rule()) {
                    break;
                }
            }
        }
    }
}
