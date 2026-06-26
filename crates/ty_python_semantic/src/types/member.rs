use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;
use ty_python_core::{place_table, scope::ScopeId, symbol::ScopedSymbolId, use_def_map};

use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    TypeOrigin, place_by_id, place_from_bindings, place_from_runtime_bindings,
};
use crate::reachability::evaluate_reachability_runtime;
use crate::types::Type;

/// The return type of certain member-lookup operations. Contains information
/// about the type, type qualifiers, boundness/declaredness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update, get_size2::GetSize, Default)]
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

#[salsa::tracked]
fn definition_is_runtime_binding<'db>(db: &'db dyn Db, definition: Definition<'db>) -> bool {
    let file = definition.file(db);
    let module = parsed_module(db, file).load(db);
    definition
        .kind(db)
        .category(file.is_stub(db), &module)
        .is_binding()
}

fn declaration_is_runtime_reachable<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    symbol_id: ScopedSymbolId,
    definition: Definition<'db>,
) -> bool {
    let use_def = use_def_map(db, scope);
    let declarations = use_def.end_of_scope_symbol_declarations(symbol_id);
    let predicates = declarations.predicates();
    let reachability_constraints = declarations.reachability_constraints();

    declarations.into_iter().any(|declaration| {
        declaration
            .declaration
            .is_defined_and(|candidate| candidate == definition)
            && !evaluate_reachability_runtime(
                db,
                reachability_constraints,
                predicates,
                declaration.reachability_constraint,
            )
            .is_always_false()
    })
}

/// Infer the public type of a class member/symbol (its type as seen from outside its scope) in the given
/// `scope`.
pub(super) fn class_member<'db>(db: &'db dyn Db, scope: ScopeId<'db>, name: &str) -> Member<'db> {
    place_table(db, scope)
        .symbol_id(name)
        .map(|symbol_id| {
            let place_and_quals = place_by_id(
                db,
                scope,
                symbol_id.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            );

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
                let inferred = place_from_bindings(db, bindings).place;

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

#[salsa::tracked]
pub(super) fn runtime_class_member<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: Name,
) -> Member<'db> {
    let name = String::from(name);
    let member = class_member(db, scope, &name);
    let Some(symbol_id) = place_table(db, scope).symbol_id(&name) else {
        return member;
    };
    let runtime = place_from_runtime_bindings(
        db,
        use_def_map(db, scope).end_of_scope_symbol_bindings(symbol_id),
    )
    .place;

    let qualifiers = member.inner.qualifiers;
    let place = match (member.inner.place, runtime) {
        (
            Place::Defined(
                declared @ DefinedPlace {
                    origin: TypeOrigin::Declared,
                    provenance,
                    ..
                },
            ),
            Place::Defined(runtime),
        ) if provenance.definition().is_none_or(|definition| {
            declaration_is_runtime_reachable(db, scope, symbol_id, definition)
        }) =>
        {
            Place::Defined(DefinedPlace {
                definedness: runtime.definedness,
                ..declared
            })
        }
        (Place::Defined(declared), Place::Undefined)
            if declared.provenance.definition().is_some_and(|definition| {
                definition.file(db).is_stub(db) && definition_is_runtime_binding(db, definition)
            }) =>
        {
            Place::Defined(declared)
        }
        (_, runtime) => runtime,
    };

    Member {
        inner: place.with_qualifiers(qualifiers),
    }
}
