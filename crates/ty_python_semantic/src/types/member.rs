use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    TypeOrigin, place_by_id, place_from_bindings, place_from_declarations,
};
use crate::types::{
    ProgramEnvironment, StaticClassLiteral, Type, class::MroLookup, infer::nearest_enclosing_class,
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
    symbol
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
            {
                if let Place::Defined(declared) = inherited.member.place {
                    // The annotation determines the public type, but the value is still supplied
                    // by this class. Consumers such as Pydantic inspect that value's definition.
                    *place = DefinedPlace {
                        ty: declared.ty,
                        origin: declared.origin,
                        public_type_policy: declared.public_type_policy,
                        ..*place
                    };
                }
                place_and_quals.qualifiers = inherited.member.qualifiers;
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
) -> Option<ClassBodyDeclaration<'db>> {
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
    let mut inherited = MroLookup::new(
        db,
        &env,
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

/// Returns whether a method with this name is dispatched through special-method lookup on the type.
///
/// The names are the methods registered in CPython's `slotdefs` table or explicitly looked
/// up on the type by Python or its standard library.
pub(super) fn uses_special_method_lookup(name: &str) -> bool {
    matches!(
        name,
        "__abs__"
            | "__add__"
            | "__aenter__"
            | "__aexit__"
            | "__aiter__"
            | "__and__"
            | "__anext__"
            | "__await__"
            | "__bool__"
            | "__buffer__"
            | "__bytes__"
            | "__call__"
            | "__ceil__"
            | "__complex__"
            | "__contains__"
            | "__copy__"
            | "__del__"
            | "__delattr__"
            | "__delete__"
            | "__delitem__"
            | "__dir__"
            | "__divmod__"
            | "__enter__"
            | "__eq__"
            | "__exit__"
            | "__float__"
            | "__floor__"
            | "__floordiv__"
            | "__format__"
            | "__fspath__"
            | "__ge__"
            | "__get__"
            | "__getattr__"
            | "__getattribute__"
            | "__getitem__"
            | "__getnewargs__"
            | "__getnewargs_ex__"
            | "__gt__"
            | "__hash__"
            | "__iadd__"
            | "__iand__"
            | "__ifloordiv__"
            | "__ilshift__"
            | "__imatmul__"
            | "__imod__"
            | "__imul__"
            | "__index__"
            | "__init__"
            | "__instancecheck__"
            | "__int__"
            | "__invert__"
            | "__ior__"
            | "__ipow__"
            | "__irshift__"
            | "__isub__"
            | "__iter__"
            | "__itruediv__"
            | "__ixor__"
            | "__le__"
            | "__len__"
            | "__length_hint__"
            | "__lshift__"
            | "__lt__"
            | "__matmul__"
            | "__missing__"
            | "__mod__"
            | "__mul__"
            | "__ne__"
            | "__neg__"
            | "__new__"
            | "__next__"
            | "__or__"
            | "__pos__"
            | "__pow__"
            | "__radd__"
            | "__rand__"
            | "__rdivmod__"
            | "__release_buffer__"
            | "__replace__"
            | "__repr__"
            | "__reversed__"
            | "__rfloordiv__"
            | "__rlshift__"
            | "__rmatmul__"
            | "__rmod__"
            | "__rmul__"
            | "__ror__"
            | "__round__"
            | "__rpow__"
            | "__rrshift__"
            | "__rshift__"
            | "__rsub__"
            | "__rtruediv__"
            | "__rxor__"
            | "__set__"
            | "__set_name__"
            | "__setattr__"
            | "__setitem__"
            | "__sizeof__"
            | "__str__"
            | "__sub__"
            | "__subclasscheck__"
            | "__truediv__"
            | "__trunc__"
            | "__xor__"
    )
}
