//! Constructor `__new__` lookup through admitted subtype and MRO dependencies.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::place::PlaceAndQualifiers;
use crate::types::constructor::new_lookup::{
    NewLookupEffects, NewLookupFacts, lookup_dunder_new_with,
};
use crate::types::relation::source::{FreshRelation, resources::RelationResourceAccess};
use crate::types::{KnownClass, MemberLookupPolicy, Type};
use crate::ProgramEnvironment;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Resolves the canonical `__new__` query body without binding its descriptor.
    pub(in crate::types::infer) async fn infer_constructor_new(
        &self,
        ty: Type<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        let env = self
            .initialize_value(|| ProgramEnvironment::from_program(self.program))
            .await?;
        let member = self.allocate_future(|| lookup_dunder_new_with(ty, &env, NewLookupFacts, self))
            .await?
            .await?;
        self.initialize_value(|| member).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NewLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local(
            4,
            Self::checked(
                size_of::<Type<'db>>()
                    .checked_add(size_of::<bool>())
                    .and_then(|bytes| bytes.checked_add(size_of::<MemberLookupPolicy>())),
            )?,
            || (),
        )
        .await
    }

    async fn type_instance(&self, _env: &ProgramEnvironment<'db>) -> RunResult<Type<'db>> {
        self.access
            .known_class_instance(self.program, KnownClass::Type)
            .await
    }

    async fn is_subtype(
        &self,
        ty: Type<'db>,
        target: Type<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<bool> {
        let resources = self
            .local(
                4,
                Self::checked(
                    size_of::<Type<'db>>()
                        .checked_mul(2)
                        .and_then(|bytes| bytes.checked_add(size_of::<A::Resources>()))
                        .and_then(|bytes| bytes.checked_add(size_of::<FreshRelation>())),
                )?,
                || self.access.resources(),
            )
            .await?;
        resources
            .condition(self.db(), env, ty, target, FreshRelation::Subtyping, self)
            .await
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.local(
            2,
            Self::checked(size_of::<Type<'db>>().checked_add(size_of::<MemberLookupPolicy>()))?,
            || (),
        )
        .await?;
        let member = self.source_find_name_in_mro(ty, "__new__", policy).await?;
        self.initialize_value(|| member).await
    }
}
