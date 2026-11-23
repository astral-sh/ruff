use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    TypeOrigin, place_by_id, place_from_bindings, place_from_declarations,
};
use crate::types::{
    ClassType, DivergentFlags, DynamicType, ProgramEnvironment, StaticClassLiteral, Type,
    TypeQualifiers, UnionType, UnknownOrigin,
    class::MroLookup,
    infer::{infer_class_initializer, nearest_enclosing_class},
};
use ty_python_core::{
    place_table, scope::ScopeId, semantic_index, symbol::ScopedSymbolId, use_def_map,
};

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

/// A class-body annotation together with the class that declares it.
///
/// The declaring class is retained even when several declarations contribute to the member's
/// type, or an unannotated default inherits the annotation through an intermediate base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ClassBodyDeclaration<'db> {
    pub(super) member: PlaceAndQualifiers<'db>,
    pub(super) class: StaticClassLiteral<'db>,
}

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    class_member_with_symbol(db, scope, place_table(db, scope).symbol_id(name))
}

pub(super) fn class_member_with_symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: Option<ScopedSymbolId>,
) -> Member<'db> {
    symbol.map_or_default(|symbol_id| {
        let mut place_and_quals = place_by_id(
            db,
            scope,
            symbol_id.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        );

        if let Place::Defined(ref mut place) = place_and_quals.place
            && place.origin == TypeOrigin::Inferred
        {
            apply_inherited_declaration(
                db,
                scope,
                symbol_id,
                place,
                &mut place_and_quals.qualifiers,
            );
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
}

/// Use the inherited annotation for a class-body assignment, retaining the initializer
/// as a lower bound until a recursive reference to the subclass resolves.
fn apply_inherited_declaration<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
    place: &mut DefinedPlace<'db>,
    qualifiers: &mut TypeQualifiers,
) {
    // Looking up the binding or the assignment first can re-enter member lookup
    // before the initializer has been inferred. Infer it without the declaration.
    if is_unresolved_member(place.ty)
        && let Some(definition) = place.provenance.definition().or_else(|| {
            // A provisional place has no provenance. Only use a unique binding.
            let mut bindings = use_def_map(db, scope).end_of_scope_symbol_bindings(symbol);
            let definition = bindings.next()?.binding.definition()?;
            bindings.next().is_none().then_some(definition)
        })
        && definition.scope(db) == scope
    {
        let initializer = infer_class_initializer(db, definition);
        place.ty = provisional_member_type(
            db,
            &ProgramEnvironment::from_scope(scope),
            initializer,
            place.ty,
        );
    }

    let Some(inherited) = inherited_class_body_declaration(db, scope, symbol) else {
        return;
    };
    *qualifiers = inherited.member.qualifiers;
    let Place::Defined(declared) = inherited.member.place else {
        return;
    };

    // For `x = 1`, a writable `x: int | None` is still possible, but `x: str` is not.
    // An unrelated invalid reference or an omitted argument supplies no such constraint.
    let unresolved = match declared.ty {
        Type::Dynamic(DynamicType::Unknown(UnknownOrigin::InvalidClassMember(owner))) => {
            owner == scope
        }
        ty => is_unresolved_member(ty),
    };
    let ty = if unresolved {
        provisional_member_type(
            db,
            &ProgramEnvironment::from_scope(scope),
            place.ty,
            declared.ty,
        )
    } else {
        declared.ty
    };
    // The annotation determines the public type, but the value is still supplied
    // by this class. Consumers such as Pydantic inspect that value's definition.
    *place = DefinedPlace {
        ty,
        origin: declared.origin,
        public_type_policy: declared.public_type_policy,
        ..*place
    };
}

/// A recursive alias or a materialized bound conveys more than an unresolved member.
fn is_unresolved_member(ty: Type<'_>) -> bool {
    matches!(ty, Type::Divergent(d) if ty.is_recursive_divergent()
        && !d.flags.contains(DivergentFlags::FROM_TYPE_ALIAS)
        && d.materialization.is_none())
}

/// Keep context-independent information from the initializer while the annotation is unresolved.
fn provisional_member_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    initializer: Type<'db>,
    pending: Type<'db>,
) -> Type<'db> {
    let lower_bound = match initializer {
        Type::LiteralValue(literal) => Type::LiteralValue(literal.to_unpromotable()),
        Type::NominalInstance(instance) => match instance.class(db, env) {
            // For example, `[1]` can initialize a `list[int | None]`.
            ClassType::Generic(alias) => {
                Type::instance(db, env, alias.origin(db).unknown_specialization(db))
            }
            ClassType::NonGeneric(_) => initializer,
        },
        Type::Union(u) => {
            return u.map(db, env, |ty| provisional_member_type(db, env, *ty, pending));
        }
        // A lambda needs the annotation to infer its signature. Its provisional
        // function type can also bind as a method when accessed on an instance.
        Type::Callable(callable) if pending.is_divergent() && callable.is_method_like(db) => {
            return pending;
        }
        Type::Dynamic(_) | Type::Divergent(_) => return pending,
        _ => initializer,
    };
    UnionType::from_elements(db, env, [lower_bound, pending])
}

/// Returns the inherited class-body annotation governing an unannotated class attribute.
///
/// A subclass assignment such as `items = []` retains an inherited `items: list[int]`
/// declaration. Both initializer inference and public member lookup use that declaration,
/// while an explicit annotation or a new method definition supplies its own public type.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn inherited_class_body_declaration<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol: ScopedSymbolId,
) -> Option<ClassBodyDeclaration<'db>> {
    scope.node(db).as_class()?;
    let use_def = use_def_map(db, scope);
    let env = ProgramEnvironment::from_scope(scope);
    if !place_from_declarations(db, &env, use_def.end_of_scope_symbol_declarations(symbol))
        .ignore_conflicting_declarations()
        .is_undefined()
    {
        return None;
    }

    let class = nearest_enclosing_class(db, semantic_index(db, scope.program_file(db)), scope)?;
    let table = place_table(db, scope);
    class_body_declaration_from_bases(db, &env, class, table.symbol(symbol).name())
}

pub(super) fn class_body_declaration_from_bases<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    name: &str,
) -> Option<ClassBodyDeclaration<'db>> {
    let mut inherited = MroLookup::new(
        db,
        env,
        class.identity_specialization(db).iter_mro(db).skip(1),
    )
    .class_body_declaration(name)?;
    // Hash definitions are checked against the hash protocol, not inherited annotations.
    // Retain qualifiers such as `ClassVar`, which still constrain instance assignments.
    if name == "__hash__" {
        inherited.member.place = Place::Undefined;
    }
    Some(inherited)
}
