use crate::Db;
use crate::place::Place;
use crate::place::PlaceAndQualifiers;
use crate::types::Type;
use crate::types::class::member_source::{
    InlineMemberSourceEffects, RawClassMemberFacts, raw_class_member_sync,
};
use ty_python_core::scope::ScopeId;

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
    match raw_class_member_sync(
        scope,
        name,
        RawClassMemberFacts,
        &InlineMemberSourceEffects::new(db),
    ) {
        Ok(member) => member,
        Err(never) => match never {},
    }
}
