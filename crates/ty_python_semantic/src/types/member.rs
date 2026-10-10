use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Definedness, Place, PlaceAndQualifiers,
    RequiresExplicitReExport, TypeOrigin, place_by_id, place_from_bindings,
    place_from_declarations,
};
use crate::types::{
    ClassBase, ClassType, KnownInstanceType, MemberLookupKey, MemberLookupPolicy,
    ModuleLiteralType, ProgramEnvironment, PropertyInstanceClass, Type, TypeVarBoundOrConstraints,
    class::{CodeGeneratorKind, MroLookup},
    infer::nearest_enclosing_class,
};
use ty_python_core::{
    ProgramFile,
    definition::{DefinitionKind, DefinitionState},
    global_scope, place_table,
    scope::ScopeId,
    semantic_index,
    symbol::ScopedSymbolId,
    use_def_map,
};

/// Whether module members, class bindings, or protocol methods establish an attribute's presence.
///
/// Instance annotations and assignments do not establish presence because we do not check
/// definite initialization. For instance access, descriptors other than ordinary methods can
/// raise `AttributeError`, even when the descriptor itself is bound on the class.
pub(super) fn has_definitely_present_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    name: &str,
) -> bool {
    has_definitely_present_attribute_impl(
        db,
        MemberLookupKey::new(db, env.program(db), ty, name, MemberLookupPolicy::default()),
    )
}

#[salsa::tracked(returns(copy), cycle_result=|_, _, _| false, heap_size=ruff_memory_usage::heap_size)]
fn has_definitely_present_attribute_impl<'db>(db: &'db dyn Db, key: MemberLookupKey<'db>) -> bool {
    let env = ProgramEnvironment::from_program(key.program(db));
    let name = key.name(db).as_str();
    let has_attribute = |ty| has_definitely_present_attribute(db, &env, ty, name);

    match key.ty(db) {
        Type::NominalInstance(instance) => has_definitely_bound_class_attribute(
            db,
            &env,
            instance.class(db, &env),
            name,
            ClassAttributeAccess::Instance,
        ),
        Type::ModuleLiteral(module) => {
            has_definitely_present_module_attribute(db, &env, module, name)
        }
        ty @ (Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)) => {
            has_definitely_present_class_object_attribute(db, &env, ty, name)
        }
        Type::ProtocolInstance(protocol) => protocol
            .interface(db)
            .member_by_name(db, name)
            .is_some_and(|member| member.is_method()),
        Type::Union(union) => union.elements(db).iter().copied().all(has_attribute),
        Type::Intersection(intersection) => intersection.iter_positive(db).any(has_attribute),
        Type::TypeVar(typevar) => match typevar.typevar(db).bound_or_constraints(db, &env) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => has_attribute(bound),
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                constraints.elements(db).iter().copied().all(has_attribute)
            }
            None => false,
        },
        Type::NewTypeInstance(newtype) => has_attribute(newtype.concrete_base_type(db)),
        Type::TypeAlias(alias) => has_attribute(alias.value_type(db)),
        Type::Recursive(recursive) => recursive
            .unfold(db, &env)
            .map(has_attribute)
            .unwrap_or(false),
        Type::LiteralValue(literal) => has_attribute(literal.fallback_instance(db, &env)),
        _ => false,
    }
}

fn has_definitely_present_module_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    module: ModuleLiteralType<'db>,
    name: &str,
) -> bool {
    let Some(file) = module.module(db).file(db) else {
        return false;
    };
    let scope = global_scope(db, ProgramFile::new(db, file, env.program(db)));
    let Some(symbol) = place_table(db, scope).symbol_id(name) else {
        return false;
    };

    // Explicit stub declarations describe a module's public interface. In Python source,
    // only bindings establish presence: a bare annotation does not initialize a global.
    // Avoid ordinary member lookup, whose ModuleType and `__getattr__` fallbacks can provide
    // attributes such as `__path__` or `__file__` that this module does not actually have.
    if file.is_stub(db) {
        place_by_id(
            db,
            scope,
            symbol.into(),
            RequiresExplicitReExport::Yes,
            ConsideredDefinitions::EndOfScope,
        )
        .place
        .is_definitely_bound()
    } else {
        place_from_bindings(
            db,
            env,
            use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
        )
        .place
        .is_definitely_bound()
    }
}

fn has_definitely_present_class_object_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    name: &str,
) -> bool {
    let Some(instance) = ty.to_instance_approximation(db, env) else {
        return false;
    };
    let Some(class) = instance.nominal_class(db, env) else {
        return false;
    };
    if matches!(ty, Type::SubclassOf(_)) && class.is_protocol(db) {
        // Structural implementations need not inherit the protocol's class bindings. Its
        // method contracts still apply, but properties can be implemented by instance storage.
        return has_definitely_present_attribute(db, env, instance, name);
    }

    // A data descriptor on the metaclass takes precedence over the class's own bindings.
    // Unlike a property stored in the class namespace, this getter runs during class access.
    if ty
        .class_member_with_policy(db, env, name, MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
        .ignore_possibly_undefined()
        .is_some_and(|member| {
            member.resolve_type_alias(db).is_object()
                || !member.is_definitely_non_data_descriptor(db, env)
        })
    {
        return false;
    }

    if has_definitely_bound_class_attribute(db, env, class, name, ClassAttributeAccess::Class) {
        return true;
    }
    if !class
        .class_member(db, env, name, MemberLookupPolicy::REQUIRE_CONCRETE)
        .place
        .is_undefined()
    {
        // An uncertain class binding can shadow an otherwise safe metaclass attribute.
        return false;
    }
    ty.to_meta_type(db, env)
        .to_instance_approximation(db, env)
        .and_then(|metaclass| metaclass.nominal_class(db, env))
        .is_some_and(|metaclass| {
            has_definitely_bound_class_attribute(
                db,
                env,
                metaclass,
                name,
                ClassAttributeAccess::Instance,
            )
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassAttributeAccess {
    Instance,
    Class,
}

fn has_definitely_bound_class_attribute<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    name: &str,
    access: ClassAttributeAccess,
) -> bool {
    for base in class.iter_mro(db) {
        let class = match base {
            ClassBase::Class(class) => class,
            ClassBase::Generic | ClassBase::Protocol => continue,
            _ => return false,
        };
        let Some((class, _)) = class.static_class_literal(db) else {
            return false;
        };
        if class.has_own_slot_descriptor(db, name) {
            return access == ClassAttributeAccess::Class;
        }

        let scope = class.body_scope(db);
        let Some(symbol) = place_table(db, scope).symbol_id(name) else {
            continue;
        };
        if let Some(
            field_policy @ (CodeGeneratorKind::DataclassLike(_) | CodeGeneratorKind::Pydantic(_)),
        ) = CodeGeneratorKind::from_class(db, class.into())
            && class.own_fields(db, None, field_policy).contains_key(name)
        {
            // Field transformations can remove the class binding or initialize the value only
            // on instances. The original field specifier is not evidence of runtime presence.
            return false;
        }
        let use_def = use_def_map(db, scope);
        let mut has_binding = false;
        let mut has_bare_annotation = false;
        for binding in use_def.end_of_scope_symbol_bindings(symbol) {
            let DefinitionState::Defined(definition) = binding.binding else {
                continue;
            };
            if matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(assignment) if !assignment.has_value())
            {
                // Stub annotations participate in binding inference, but do not describe an
                // initialized class attribute. They can coexist with conditional bindings.
                has_bare_annotation = true;
            } else {
                has_binding = true;
            }
        }
        if !has_binding {
            continue;
        }
        if has_bare_annotation {
            return false;
        }
        let Place::Defined(binding) =
            place_from_bindings(db, env, use_def.end_of_scope_symbol_bindings(symbol)).place
        else {
            continue;
        };
        if access == ClassAttributeAccess::Class
            && (matches!(binding.ty, Type::SlotDescriptor(_))
                || matches!(binding.ty, Type::PropertyInstance(property) if matches!(property.instance_class(db), PropertyInstanceClass::Builtin)))
        {
            // Class access returns the descriptor itself without reading instance storage or
            // invoking a property getter.
            return binding.definedness == Definedness::AlwaysDefined;
        }
        return binding.definedness == Definedness::AlwaysDefined
            // An `object` return annotation can hide a descriptor supplied by a factory.
            && !binding.ty.resolve_type_alias(db).is_object()
            && binding.ty.is_definitely_non_data_descriptor(db, env)
            && (binding.ty.function_like_kind(db).is_some()
                && !matches!(binding.ty, Type::KnownInstance(KnownInstanceType::MethodWrapper(_)))
                || binding
                    .ty
                    .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
                    .place
                    .is_undefined());
    }
    false
}

/// The return type of certain member-lookup operations. Contains information
/// about the type, type qualifiers, boundness/declaredness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, Default, salsa::SalsaValue)]
pub(super) struct Member<'db> {
    /// Type, qualifiers, and boundness information of this member
    pub(super) inner: PlaceAndQualifiers<'db>,
}

impl<'db> Member<'db> {
    pub(super) fn unbound() -> Self {
        Self {
            inner: PlaceAndQualifiers::unbound(),
        }
    }

    pub(super) fn definitely_declared(ty: Type<'db>) -> Self {
        Self {
            inner: Place::declared(ty).into(),
        }
    }

    /// Returns the type qualifiers of this member.
    pub(super) fn qualifiers(&self) -> crate::types::TypeQualifiers {
        self.inner.qualifiers
    }

    /// Returns `true` if the inner place is undefined (i.e. there is no such member).
    pub(super) fn is_undefined(&self) -> bool {
        self.inner.place.is_undefined()
    }

    /// Returns the inner type, unless it is definitely undefined.
    pub(super) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        self.inner.place.ignore_possibly_undefined()
    }

    /// Map a type transformation function over the type of this member.
    #[must_use]
    pub(super) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            inner: self.inner.map_type(f),
        }
    }
}

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    place_table(db, scope)
        .symbol_id(name)
        .map(|symbol_id| {
            let mut place_and_quals = place_by_id(
                db,
                scope,
                symbol_id.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            );

            if let Place::Defined(ref mut place) = place_and_quals.place
                && place.origin == TypeOrigin::Inferred
                && let Some(inherited) = inherited_class_body_declaration(db, scope, symbol_id)
                && let Place::Defined(declared) = inherited.place
            {
                // The annotation determines the public type, but the value is still supplied
                // by this class. Consumers such as Pydantic inspect that value's definition.
                *place = DefinedPlace {
                    ty: declared.ty,
                    origin: declared.origin,
                    public_type_policy: declared.public_type_policy,
                    ..*place
                };
                place_and_quals.qualifiers = inherited.qualifiers;
            }

            if !place_and_quals.is_undefined() && !place_and_quals.is_init_var() {
                // Trust the declared type if we see a class-level declaration
                return Member {
                    inner: place_and_quals,
                };
            }

            if let PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty,
                        provenance: declared_provenance,
                        ..
                    }),
                qualifiers,
            } = place_and_quals
            {
                // Otherwise, we need to check if the symbol has bindings
                let use_def = use_def_map(db, scope);
                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                let env = ProgramEnvironment::from_scope(scope);
                let inferred = place_from_bindings(db, &env, bindings).place;

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                Member {
                    inner: match inferred {
                        Place::Undefined => Place::Undefined.with_qualifiers(qualifiers),
                        Place::Defined(place) => Place::Defined(DefinedPlace {
                            ty,
                            provenance: place.provenance.or(declared_provenance),
                            ..place
                        })
                        .with_qualifiers(qualifiers),
                    },
                }
            } else {
                Member::unbound()
            }
        })
        .unwrap_or_default()
}

/// Returns the inherited class-body annotation governing an unannotated class attribute.
///
/// A subclass assignment such as `items = []` retains an inherited `items: list[int]`
/// declaration. Both initializer inference and public member lookup use that declaration,
/// while an explicit annotation or a new method definition supplies its own public type.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn inherited_class_body_declaration<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Option<PlaceAndQualifiers<'db>> {
    scope.node(db).as_class()?;
    let table = place_table(db, scope);
    let name = table.symbol(symbol).name();
    let use_def = use_def_map(db, scope);
    let env = ProgramEnvironment::from_scope(scope);
    if !place_from_declarations(db, &env, use_def.end_of_scope_symbol_declarations(symbol))
        .ignore_conflicting_declarations()
        .is_undefined()
    {
        return None;
    }

    let class = nearest_enclosing_class(db, semantic_index(db, scope.program_file(db)), scope)?;
    MroLookup::new(
        db,
        &env,
        class.identity_specialization(db).iter_mro(db).skip(1),
    )
    .class_body_declaration(name)
}
