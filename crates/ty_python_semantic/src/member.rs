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
        Member::undeclared(PlaceAndQualifiers::default())
    }
}

impl<'db> Member<'db> {
    pub(crate) fn undeclared(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: false,
        }
    }

    pub(crate) fn declared(inner: PlaceAndQualifiers<'db>) -> Self {
        Self {
            inner,
            is_declared: true,
        }
    }

    pub(crate) fn definitely_declared(ty: Type<'db>) -> Self {
        Self::declared(Place::bound(ty).into())
    }

    pub(crate) fn is_unbound(&self) -> bool {
        self.inner.place.is_unbound()
    }

    /// Represents the absence of a member.
    pub(crate) fn unbound() -> Self {
        Self::undeclared(PlaceAndQualifiers::default())
    }

    pub(crate) fn ignore_possibly_unbound(&self) -> Option<Type<'db>> {
        self.inner.place.ignore_possibly_unbound()
    }

    #[must_use]
    pub(crate) fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            inner: self.inner.map_type(f),
            is_declared: self.is_declared,
        }
    }
}
