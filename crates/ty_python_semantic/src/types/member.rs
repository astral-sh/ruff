use crate::Db;
use crate::place::{
    ConsideredDefinitions, Place, PlaceAndQualifiers, RequiresExplicitReExport, place_by_id,
    place_from_bindings,
};
use crate::semantic_index::{place_table, scope::ScopeId, use_def_map};
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
                place: Place::Defined(ty, _, _, _),
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
                        Place::Defined(_, origin, boundness, widening) => {
                            Place::Defined(ty, origin, boundness, widening)
                                .with_qualifiers(qualifiers)
                        }
                    },
                }
            } else {
                Member::unbound()
            }
        })
        .unwrap_or_default()
}
