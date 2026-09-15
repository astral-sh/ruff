use crate::Db;
use crate::place::{
    ConsideredDefinitions, DefinedPlace, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    place_by_id, place_from_bindings,
};
use crate::types::protocol_class::ProtocolMemberType;
use crate::types::{ProgramEnvironment, Type};
use ty_python_core::{place_table, scope::ScopeId, use_def_map};

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

/// A member together with any binding information that is not represented by its value type.
pub(super) type LookupMember<'db> = ProtocolMemberType<'db, PlaceAndQualifiers<'db>>;

impl<'db> LookupMember<'db> {
    pub(super) fn from_attribute(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Self {
        if let Place::Defined(place) = member.place
            && place.ty.supports_self_binding(db, env)
        {
            Self::with_attribute_definition(db, member, place.provenance.definition())
        } else {
            Self::new(member)
        }
    }

    /// Expose the member without binding it to a receiver, as required by raw member lookups.
    pub(super) fn into_place(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.map_type(|member| member.resolve(db, env).map(ProtocolMemberType::ty))
    }

    pub(super) fn bind_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        self_type: Type<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.map_type(|member| match member {
            ProtocolMemberType::Value {
                ty,
                self_binding_context,
            } if self_binding_context.is_none() || !ty.supports_self_binding(db, env) => Some(ty),
            _ => member.bind_self_with_receiver(db, env, receiver, self_type),
        })
    }

    fn map_type(
        self,
        f: impl FnOnce(ProtocolMemberType<'db>) -> Option<Type<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        let member = self.into_inner();
        let Some(ty) = member.place.ignore_possibly_undefined() else {
            return member;
        };
        match f(self.map_value(|_| ty)) {
            Some(ty) => member.map_type(|_| ty),
            None => Place::Undefined.with_qualifiers(member.qualifiers),
        }
    }

    pub(super) fn or_fall_back_to(self, fallback: Self) -> ClassObjectMember<'db> {
        ClassObjectMember {
            primary: self,
            fallback: Some(fallback),
        }
    }
}

/// Class-namespace candidates in lookup order, before receiver binding.
///
/// Keep a metaclass-provided value separate from a protocol classmethod until binding is
/// complete. Combining their types first would lose which callable needs the class receiver.
#[derive(Debug, Clone, Copy)]
pub(super) struct ClassObjectMember<'db> {
    primary: LookupMember<'db>,
    fallback: Option<LookupMember<'db>>,
}

impl<'db> ClassObjectMember<'db> {
    pub(super) fn into_place(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.map_members(db, env, |member| member.into_place(db, env))
    }

    pub(super) fn bind_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        self_type: Type<'db>,
    ) -> PlaceAndQualifiers<'db> {
        self.map_members(db, env, |member| {
            member.bind_receiver(db, env, receiver, self_type)
        })
    }

    fn map_members(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut f: impl FnMut(LookupMember<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let primary = f(self.primary);
        match self.fallback {
            Some(fallback) => primary.or_fall_back_to(db, env, || f(fallback)),
            None => primary,
        }
    }
}

impl<'db> From<LookupMember<'db>> for ClassObjectMember<'db> {
    fn from(primary: LookupMember<'db>) -> Self {
        Self {
            primary,
            fallback: None,
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
