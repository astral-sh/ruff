//! Canonical enum-class construction in metadata order.

use std::collections::hash_map;
use std::convert::Infallible;

use ruff_python_ast::name::Name;
#[cfg(feature = "experimental-analysis")]
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{EnumClassLiteral, EnumMetadata, enum_metadata};
use crate::types::{ClassLiteral, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct EnumMemberCursor<'db> {
    names: indexmap::map::Keys<'db, Name, Type<'db>>,
}

impl<'db> EnumMemberCursor<'db> {
    pub(in crate::types) fn new(metadata: &'db EnumMetadata<'db>) -> Self {
        Self {
            names: metadata.members.keys(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<&'db Name> {
        self.names.next()
    }
}

pub(in crate::types) struct EnumAliasCursor<'db> {
    aliases: hash_map::Iter<'db, Name, Name>,
}

impl<'db> EnumAliasCursor<'db> {
    pub(in crate::types) fn new(metadata: &'db EnumMetadata<'db>) -> Self {
        Self {
            aliases: metadata.aliases.iter(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<(&'db Name, &'db Name)> {
        self.aliases.next()
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousEnumClassEffects)]
    pub(in crate::types) trait EnumClassEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn environment(&self, class: ClassLiteral<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(child)]
        async fn metadata(&self, class: ClassLiteral<'db>) -> Result<Option<&'db EnumMetadata<'db>>, Self::Error>;
        #[operation(local)]
        async fn members(&self, metadata: &'db EnumMetadata<'db>) -> Result<Vec<(Name, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn member_cursor(&self, metadata: &'db EnumMetadata<'db>) -> Result<EnumMemberCursor<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_member(&self, cursor: &mut EnumMemberCursor<'db>) -> Result<Option<&'db Name>, Self::Error>;
        #[operation(child)]
        async fn value_type(&self, metadata: &'db EnumMetadata<'db>, env: &ProgramEnvironment<'db>, name: &'db Name) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn push_member(&self, members: &mut Vec<(Name, Type<'db>)>, name: &'db Name, value: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn box_members(&self, members: Vec<(Name, Type<'db>)>) -> Result<Box<[(Name, Type<'db>)]>, Self::Error>;
        #[operation(local)]
        async fn aliases(&self, metadata: &'db EnumMetadata<'db>) -> Result<Vec<(Name, Name)>, Self::Error>;
        #[operation(local)]
        async fn alias_cursor(&self, metadata: &'db EnumMetadata<'db>) -> Result<EnumAliasCursor<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_alias(&self, cursor: &mut EnumAliasCursor<'db>) -> Result<Option<(&'db Name, &'db Name)>, Self::Error>;
        #[operation(local)]
        async fn push_alias(&self, aliases: &mut Vec<(Name, Name)>, alias: &'db Name, member: &'db Name) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn sort_aliases(&self, aliases: &mut [(Name, Name)]) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn box_aliases(&self, aliases: Vec<(Name, Name)>) -> Result<Box<[(Name, Name)]>, Self::Error>;
        #[operation(local)]
        async fn metaclass_may_transform_values(&self, metadata: &'db EnumMetadata<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_flag_subtype(&self, class: ClassLiteral<'db>, env: &ProgramEnvironment<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn aliases_are_known(&self, metadata: &'db EnumMetadata<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn intern(&self, class: ClassLiteral<'db>, members: Box<[(Name, Type<'db>)]>, aliases: Box<[(Name, Name)]>, aliases_are_known: bool, members_are_exhaustive: bool) -> Result<EnumClassLiteral<'db>, Self::Error>;
    }

    #[synchronous(enum_class_literal_sync)]
    #[capabilities(effects = EnumClassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn enum_class_literal_with<'db, E: EnumClassEffects<'db>>(
        class: ClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<EnumClassLiteral<'db>>, E::Error> {
        effects.checkpoint().await?;
        let env = effects.environment(class).await?;
        let Some(metadata) = effects.metadata(class).await? else {
            return Ok(None);
        };
        let mut members = effects.members(metadata).await?;
        let mut names = effects.member_cursor(metadata).await?;
        #[cursor_loop]
        while let Some(name) = effects.next_member(&mut names).await? {
            let Some(value) = effects.value_type(metadata, &env, name).await? else {
                return Ok(None);
            };
            effects.push_member(&mut members, name, value).await?;
        }
        let members = effects.box_members(members).await?;
        let mut aliases = effects.aliases(metadata).await?;
        let mut alias_names = effects.alias_cursor(metadata).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_alias(&mut alias_names).await? {
            let (alias, member) = entry;
            effects.push_alias(&mut aliases, alias, member).await?;
        }
        effects.sort_aliases(&mut aliases).await?;
        let members_are_exhaustive = !effects.metaclass_may_transform_values(metadata).await?
            && !effects.is_flag_subtype(class, &env).await?;
        let aliases = effects.box_aliases(aliases).await?;
        let aliases_are_known = effects.aliases_are_known(metadata).await?;
        Ok(Some(effects.intern(class, members, aliases, aliases_are_known, members_are_exhaustive).await?))
    }
}

pub(super) struct InlineEnumClassEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineEnumClassEffects<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousEnumClassEffects<'db> for InlineEnumClassEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn environment(&self, class: ClassLiteral<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_file(class.program_file(self.db)))
    }

    fn metadata(
        &self,
        class: ClassLiteral<'db>,
    ) -> Result<Option<&'db EnumMetadata<'db>>, Infallible> {
        Ok(enum_metadata(self.db, class))
    }

    fn members(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> Result<Vec<(Name, Type<'db>)>, Infallible> {
        Ok(Vec::with_capacity(metadata.members.len()))
    }

    fn member_cursor(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> Result<EnumMemberCursor<'db>, Infallible> {
        Ok(EnumMemberCursor::new(metadata))
    }

    fn next_member(
        &self,
        cursor: &mut EnumMemberCursor<'db>,
    ) -> Result<Option<&'db Name>, Infallible> {
        Ok(cursor.next())
    }

    fn value_type(
        &self,
        metadata: &'db EnumMetadata<'db>,
        env: &ProgramEnvironment<'db>,
        name: &'db Name,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(metadata.value_type(self.db, env, name))
    }

    fn push_member(
        &self,
        members: &mut Vec<(Name, Type<'db>)>,
        name: &'db Name,
        value: Type<'db>,
    ) -> Result<(), Infallible> {
        members.push((name.clone(), value));
        Ok(())
    }

    fn box_members(
        &self,
        members: Vec<(Name, Type<'db>)>,
    ) -> Result<Box<[(Name, Type<'db>)]>, Infallible> {
        Ok(members.into_boxed_slice())
    }

    fn aliases(&self, metadata: &'db EnumMetadata<'db>) -> Result<Vec<(Name, Name)>, Infallible> {
        Ok(Vec::with_capacity(metadata.aliases.len()))
    }

    fn alias_cursor(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> Result<EnumAliasCursor<'db>, Infallible> {
        Ok(EnumAliasCursor::new(metadata))
    }

    fn next_alias(
        &self,
        cursor: &mut EnumAliasCursor<'db>,
    ) -> Result<Option<(&'db Name, &'db Name)>, Infallible> {
        Ok(cursor.next())
    }

    fn push_alias(
        &self,
        aliases: &mut Vec<(Name, Name)>,
        alias: &'db Name,
        member: &'db Name,
    ) -> Result<(), Infallible> {
        aliases.push((alias.clone(), member.clone()));
        Ok(())
    }

    fn sort_aliases(&self, aliases: &mut [(Name, Name)]) -> Result<(), Infallible> {
        aliases.sort_unstable();
        Ok(())
    }

    fn box_aliases(&self, aliases: Vec<(Name, Name)>) -> Result<Box<[(Name, Name)]>, Infallible> {
        Ok(aliases.into_boxed_slice())
    }

    fn metaclass_may_transform_values(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> Result<bool, Infallible> {
        Ok(metadata.value_construction.metaclass_may_transform_values)
    }

    fn is_flag_subtype(
        &self,
        class: ClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<bool, Infallible> {
        Ok(Type::ClassLiteral(class).is_subtype_of(
            self.db,
            env,
            KnownClass::Flag.to_subclass_of(self.db, env),
        ))
    }

    fn aliases_are_known(&self, metadata: &'db EnumMetadata<'db>) -> Result<bool, Infallible> {
        Ok(metadata.aliases_are_known)
    }

    fn intern(
        &self,
        class: ClassLiteral<'db>,
        members: Box<[(Name, Type<'db>)]>,
        aliases: Box<[(Name, Name)]>,
        aliases_are_known: bool,
        members_are_exhaustive: bool,
    ) -> Result<EnumClassLiteral<'db>, Infallible> {
        Ok(EnumClassLiteral::new(
            self.db,
            class,
            members,
            aliases,
            aliases_are_known,
            members_are_exhaustive,
        ))
    }
}

#[cfg(feature = "experimental-analysis")]
impl salsa::plumbing::interned::FiniteInternedConfiguration for EnumClassLiteral<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Hashing and equality inspect retained names and inline Type payloads. Dropping the
        // fields releases the boxed slices and shared names without traversing semantic data.
        enum_class_field_work(&fields.1, &fields.2, |_| Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        enum_class_field_work(&fields.1, &fields.2, |units| fuel.consume(units))
    }
}

#[cfg(feature = "experimental-analysis")]
fn enum_class_field_work(
    members: &[(Name, Type<'_>)],
    aliases: &[(Name, Name)],
    mut consume: impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    consume(1)?;
    consume(members.len())?;
    let mut work = 5usize;
    for (name, value) in members {
        work = work
            .checked_add(2)
            .and_then(|work| work.checked_add(name.len()))
            .and_then(|work| work.checked_add(value.inline_payload_bytes()))
            .ok_or(QuoteError::Overflow)?;
    }
    consume(aliases.len())?;
    for (alias, member) in aliases {
        work = work
            .checked_add(2)
            .and_then(|work| work.checked_add(alias.len()))
            .and_then(|work| work.checked_add(member.len()))
            .ok_or(QuoteError::Overflow)?;
    }
    Ok(work)
}

#[cfg(feature = "experimental-analysis")]
pub(in crate::types) fn register_enum_class_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut salsa::execution_probe::RegistryBuilder<'run, 'db>,
) -> salsa::execution_probe::RunResult<
    salsa::execution_probe::InternedValues<'db, EnumClassLiteral<'static>, ()>,
> {
    registry.finite_interned_values_with_memos(EnumClassLiteral::ingredient(db.zalsa()), ())
}
