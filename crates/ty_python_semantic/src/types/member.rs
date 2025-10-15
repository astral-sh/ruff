use super::Type;
use crate::Db;
use crate::place::{
    ConsideredDefinitions, Place, PlaceAndQualifiers, RequiresExplicitReExport, place_by_id,
    place_from_bindings,
};
use crate::semantic_index::{place_table, scope::ScopeId, use_def_map};

/// The return type of certain member-lookup operations. Contains information
/// about the type, type qualifiers, boundness/declaredness, and additional
/// metadata (e.g. whether or not the member was declared)
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct Member<'db> {
    /// Type, qualifiers, and boundness information of this member
    pub(super) inner: PlaceAndQualifiers<'db>,

    /// Whether or not this member was explicitly declared (e.g. `attr: int = 1`
    /// on the class body or `self.attr: int = 1` in a class method), or if the
    /// type was inferred (e.g. `attr = 1` on the class body or `self.attr = 1`
    /// in a class method).
    pub(super) is_declared: bool,
}

impl Default for Member<'_> {
    fn default() -> Self {
        Member::inferred(PlaceAndQualifiers::default())
    }
}

impl<'db> Member<'db> {
    /// Create a new [`Member`] whose type was inferred (rather than explicitly declared).
    pub(super) fn inferred(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: false,
        }
    }

    /// Create a new [`Member`] whose type was explicitly declared (rather than inferred).
    pub(super) fn declared(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: true,
        }
    }

    /// Create a new [`Member`] whose type was explicitly and definitively declared, i.e.
    /// there is no control flow path in which it might be possibly undeclared.
    pub(super) fn definitely_declared(ty: Type<'db>) -> Self {
        Self::declared(Place::bound(ty).into())
    }

    /// Represents the absence of a member.
    pub(super) fn unbound() -> Self {
        Self::inferred(PlaceAndQualifiers::default())
    }

    /// Returns `true` if the inner place is unbound (i.e. there is no such member).
    pub(super) fn is_unbound(&self) -> bool {
        self.inner.place.is_unbound()
    }

    /// Returns the inner type, unless it is definitely unbound.
    pub(super) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        self.inner.place.ignore_possibly_unbound()
    }

    /// Map a type transformation function over the type of this member.
    #[must_use]
    pub(super) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            inner: self.inner.map_type(f),
            is_declared: self.is_declared,
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

            if !place_and_quals.place.is_unbound() && !place_and_quals.is_init_var() {
                // Trust the declared type if we see a class-level declaration
                return Member::declared(place_and_quals);
            }

            if let PlaceAndQualifiers {
                place: Place::Type(ty, _),
                qualifiers,
            } = place_and_quals
            {
                // Otherwise, we need to check if the symbol has bindings
                let use_def = use_def_map(db, scope);
                let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                let inferred = place_from_bindings(db, bindings);

                // TODO: we should not need to calculate inferred type second time. This is a temporary
                // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
                Member::inferred(match inferred {
                    Place::Unbound => Place::Unbound.with_qualifiers(qualifiers),
                    Place::Type(_, boundness) => {
                        Place::Type(ty, boundness).with_qualifiers(qualifiers)
                    }
                })
            } else {
                Member::unbound()
            }
        })
        .unwrap_or_default()
}
