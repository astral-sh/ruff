use crate::{
    place::{Place, PlaceAndQualifiers},
    types::Type,
};

/// The return type of certain member-lookup operations. Contains information
/// about the type, type qualifiers, boundness/declaredness, and additional
/// metadata (e.g. whether or not the member was declared)
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct Member<'db> {
    /// Type, qualifiers, and boundness information of this member
    pub(crate) inner: PlaceAndQualifiers<'db>,

    /// Whether or not this member was explicitly declared (e.g. `attr: int = 1`
    /// on the class body or `self.attr: int = 1` in a class method), or if the
    /// type was inferred (e.g. `attr = 1` on the class body or `self.attr = 1`
    /// in a class method).
    pub(crate) is_declared: bool,
}

impl Default for Member<'_> {
    fn default() -> Self {
        Member::inferred(PlaceAndQualifiers::default())
    }
}

impl<'db> Member<'db> {
    /// Create a new [`Member`] whose type was inferred (rather than explicitly declared).
    pub(crate) fn inferred(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: false,
        }
    }

    /// Create a new [`Member`] whose type was explicitly declared (rather than inferred).
    pub(crate) fn declared(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: true,
        }
    }

    /// Create a new [`Member`] whose type was explicitly and definitively declared, i.e.
    /// there is no control flow path in which it might be possibly undeclared.
    pub(crate) fn definitely_declared(ty: Type<'db>) -> Self {
        Self::declared(Place::bound(ty).into())
    }

    /// Represents the absence of a member.
    pub(crate) fn unbound() -> Self {
        Self::inferred(PlaceAndQualifiers::default())
    }

    /// Returns `true` if the inner place is unbound (i.e. there is no such member).
    pub(crate) fn is_unbound(&self) -> bool {
        self.inner.place.is_unbound()
    }

    /// Returns the inner type, unless it is definitely unbound.
    pub(crate) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        self.inner.place.ignore_possibly_unbound()
    }

    /// Map a type transformation function over the type of this member.
    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            inner: self.inner.map_type(f),
            is_declared: self.is_declared,
        }
    }
}
