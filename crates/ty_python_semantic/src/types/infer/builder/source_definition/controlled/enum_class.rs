//! Enum-class construction preserves canonical query dependencies and owned buffers.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::{
    KnownClassSubclassEffects, class_default_specialization_with, interpret_class_literal_lookup,
    known_class_to_subclass_of_with,
};
use crate::types::enums::class_construction::{
    EnumAliasCursor, EnumClassEffects, EnumMemberCursor, enum_class_literal_with,
};
use crate::types::enums::{EnumClassLiteral, EnumMetadata};
use crate::types::relation::source::subtyping_condition;
use crate::types::subclass_of::{SubclassConstructionFacts, SubclassOfInner, subclass_from_with};
use crate::types::{ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_enum_class_literal(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        enum_class_literal_with(class, self).await
    }

    pub(in crate::types::infer::builder) async fn enum_class_literal_source(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        self.access.enum_class_literal(class).await
    }

    async fn enum_buffer<T>(&self, capacity: usize) -> RunResult<Vec<T>> {
        let work = Self::checked(capacity.checked_mul(3).and_then(|n| n.checked_add(4)))?;
        let bytes = Self::checked(capacity.checked_mul(size_of::<T>()))?;
        // Names share their character storage. Reserve cleanup for every possible initialized slot.
        self.local(work, bytes, || Vec::with_capacity(capacity))
            .await
    }

    async fn box_enum_buffer<T>(&self, buffer: Vec<T>) -> RunResult<Box<[T]>> {
        let (len, capacity) = self
            .local(2, 0, || (buffer.len(), buffer.capacity()))
            .await?;
        let work = Self::checked(
            capacity
                .checked_add(
                    len.checked_mul(3)
                        .ok_or(RunError::Contract("enum buffer cleanup work overflow"))?,
                )
                .and_then(|n| n.checked_add(4)),
        )?;
        let bytes = if len == capacity {
            0
        } else {
            Self::checked(len.checked_mul(size_of::<T>()))?
        };
        let mut owner = Some(buffer);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(work)?;
                if bytes != 0 {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: bytes,
                    })?;
                }
                endpoint.check_completion()?;
                let buffer = owner
                    .take()
                    .ok_or(RunError::Contract("enum buffer already consumed"))?;
                Ok(buffer.into_boxed_slice())
            })
            .await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EnumClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn environment(&self, class: ClassLiteral<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let file = self.class_file(class).await?;
        self.check_file_program(file).await?;
        self.local(3, 0, || ProgramEnvironment::from_file(file))
            .await
    }

    async fn metadata(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>> {
        self.access.enum_class_metadata(class).await
    }

    async fn members(&self, metadata: &'db EnumMetadata<'db>) -> RunResult<Vec<(Name, Type<'db>)>> {
        let len = self.local(1, 0, || metadata.members.len()).await?;
        self.enum_buffer(len).await
    }

    async fn member_cursor(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> RunResult<EnumMemberCursor<'db>> {
        self.local(1, 0, || EnumMemberCursor::new(metadata)).await
    }

    async fn next_member(
        &self,
        cursor: &mut EnumMemberCursor<'db>,
    ) -> RunResult<Option<&'db Name>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn value_type(
        &self,
        _metadata: &'db EnumMetadata<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &'db Name,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::EnumMemberValue).await
    }

    async fn push_member(
        &self,
        members: &mut Vec<(Name, Type<'db>)>,
        name: &'db Name,
        value: Type<'db>,
    ) -> RunResult<()> {
        let available = self
            .local(2, 0, || members.len() < members.capacity())
            .await?;
        if !available {
            return Err(RunError::Contract("enum member buffer capacity exhausted"));
        }
        self.local(4, 0, || members.push((name.clone(), value)))
            .await
    }

    async fn box_members(
        &self,
        members: Vec<(Name, Type<'db>)>,
    ) -> RunResult<Box<[(Name, Type<'db>)]>> {
        self.box_enum_buffer(members).await
    }

    async fn aliases(&self, metadata: &'db EnumMetadata<'db>) -> RunResult<Vec<(Name, Name)>> {
        let len = self.local(1, 0, || metadata.aliases().len()).await?;
        self.enum_buffer(len).await
    }

    async fn alias_cursor(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> RunResult<EnumAliasCursor<'db>> {
        let capacity = self.local(1, 0, || metadata.aliases().capacity()).await?;
        // Hash-map iteration may inspect empty buckets between two aliases.
        let work = Self::checked(capacity.checked_mul(2).and_then(|n| n.checked_add(4)))?;
        self.local(work, 0, || EnumAliasCursor::new(metadata)).await
    }

    async fn next_alias(
        &self,
        cursor: &mut EnumAliasCursor<'db>,
    ) -> RunResult<Option<(&'db Name, &'db Name)>> {
        self.local(1, 0, || cursor.next()).await
    }

    async fn push_alias(
        &self,
        aliases: &mut Vec<(Name, Name)>,
        alias: &'db Name,
        member: &'db Name,
    ) -> RunResult<()> {
        let available = self
            .local(2, 0, || aliases.len() < aliases.capacity())
            .await?;
        if !available {
            return Err(RunError::Contract("enum alias buffer capacity exhausted"));
        }
        self.local(5, 0, || aliases.push((alias.clone(), member.clone())))
            .await
    }

    async fn sort_aliases(&self, aliases: &mut [(Name, Name)]) -> RunResult<()> {
        let len = self.local(1, 0, || aliases.len()).await?;
        let inspection = Self::checked(len.checked_mul(3).and_then(|n| n.checked_add(1)))?;
        let max_bytes = self
            .local(inspection, 0, || {
                aliases
                    .iter()
                    .map(|(alias, member)| alias.len().checked_add(member.len()))
                    .try_fold(0, |maximum, bytes| bytes.map(|bytes| maximum.max(bytes)))
            })
            .await?;
        let comparison = Self::checked(
            max_bytes
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(8)),
        )?;
        // Sorting compares only these retained names. Include repeated byte comparisons and
        // movement of the fixed-size pairs without following any semantic handles.
        let work = Self::checked(
            len.checked_add(1)
                .and_then(|n| n.checked_mul(n))
                .and_then(|n| n.checked_mul(16))
                .and_then(|n| n.checked_mul(comparison)),
        )?;
        self.local(work, 0, || aliases.sort_unstable()).await
    }

    async fn box_aliases(&self, aliases: Vec<(Name, Name)>) -> RunResult<Box<[(Name, Name)]>> {
        self.box_enum_buffer(aliases).await
    }

    async fn metaclass_may_transform_values(
        &self,
        metadata: &'db EnumMetadata<'db>,
    ) -> RunResult<bool> {
        self.local(1, 0, || {
            metadata.value_construction.metaclass_may_transform_values
        })
        .await
    }

    async fn is_flag_subtype(
        &self,
        class: ClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<bool> {
        self.environment_program(env).await?;
        let effects = EnumFlagEffects { source: self, env };
        let flag = known_class_to_subclass_of_with(KnownClass::Flag, &effects).await?;
        subtyping_condition(self.db(), env, Type::ClassLiteral(class), flag, self).await
    }

    async fn aliases_are_known(&self, metadata: &'db EnumMetadata<'db>) -> RunResult<bool> {
        self.local(1, 0, || metadata.aliases_are_known).await
    }

    async fn intern(
        &self,
        class: ClassLiteral<'db>,
        members: Box<[(Name, Type<'db>)]>,
        aliases: Box<[(Name, Name)]>,
        aliases_are_known: bool,
        members_are_exhaustive: bool,
    ) -> RunResult<EnumClassLiteral<'db>> {
        self.access
            .intern_enum_class(
                class,
                members,
                aliases,
                aliases_are_known,
                members_are_exhaustive,
            )
            .await
    }
}

struct EnumFlagEffects<'env, 'access, 'run, 'db: 'run, A> {
    source: &'env SourceEffects<'access, 'run, 'db, A>,
    env: &'env ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassSubclassEffects<'db>
    for EnumFlagEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn lookup(&self, class: KnownClass) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let program = self.source.environment_program(self.env).await?;
        let result = self
            .source
            .access
            .known_class_lookup(program, class)
            .await?;
        self.source
            .local(1, 0, || interpret_class_literal_lookup(result))
            .await
    }

    async fn default_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_default_specialization_with(class, self.source).await
    }

    async fn subclass_of(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        subclass_from_with(
            SubclassOfInner::Class(class),
            SubclassConstructionFacts,
            self.source,
        )
        .await
    }
}
