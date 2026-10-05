//! Override validation retains real member and base collections across child requests.

use std::collections::hash_set::IntoIter;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::place::PlaceAndQualifiers;
use crate::types::class::context::explicit_class_bases_with;
use crate::types::class::identity::{ClassIdentityEffects, class_identity_specialization_with};
use crate::types::class::{
    CodeGeneratorKind, KnownClassInstanceEffects, class_default_specialization_with,
    static_code_generator_with,
};
use crate::types::class_base::ClassBaseConversion;
use crate::types::class_base::conversion::ClassBaseDependency;
use crate::types::enums::EnumMetadata;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation, storage,
};
use crate::types::list_members::MemberWithDefinition;
use crate::types::local_transfer::collections::CALL_3;
use crate::types::member_lookup::general::{
    GeneralMemberFacts, GeneralMemberName, member_lookup_entry_with,
};
use crate::types::mro::base::class_mro_start_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::overrides::OverrideRulesConfig;
use crate::types::overrides::inherited_selection::{
    InheritedBaseSelection, InheritedBaseSelectionEffects, classify_inherited_base_with,
    next_inherited_explicit_base, select_inherited_direct_bases_with,
};
use crate::types::overrides::member_entry::{
    OverrideGeneratorFacts, OverrideGeneratorWork, OverrideLookupEffects, OverrideLookupFacts,
    OverrideMemberEffects, OverrideMemberRequest, ResolvedOverrideMemberEffects,
    check_class_declaration_with, check_resolved_class_declaration_with, lookup_override_member_with,
};
use crate::types::overrides::validation::{
    GenericOverrideBases, OverrideCheckEffects, next_override_base,
};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, GenericAlias, GenericContext, MemberLookupPolicy, ResolvedMember,
    Specialization, StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn append_override_base<T: Copy>(&self, values: &mut Vec<T>, value: T) -> RunResult<()> {
        let (len, capacity) = self
            .source
            .local(2, 0, || (values.len(), values.capacity()))
            .await?;
        let quote = storage::sequence_merge::<T>(len, capacity, 1)
            .ok_or(RunError::Contract("override base storage quote overflow"))?;
        let (retirement_work, bytes) = if len == capacity {
            let capacity_bound = SourceEffects::<A>::checked(capacity.checked_mul(2))?
                .max(SourceEffects::<A>::checked(len.checked_add(1))?)
                .max(8);
            (
                capacity_bound,
                SourceEffects::<A>::checked(capacity_bound.checked_mul(size_of::<T>()))?,
            )
        } else {
            (0, 0)
        };
        // Growth prepays retirement of the new backing on every exit. Appending a Copy value
        // within that backing needs only constant work; its retirement is already covered.
        let work = SourceEffects::<A>::checked(
            quote
                .work
                .checked_add(retirement_work)
                .and_then(|n| n.checked_add(3)),
        )?;
        self.source
            .local(work, bytes, || {
                values.reserve(1);
                values.push(value);
            })
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OverrideCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn configuration(&self) -> RunResult<OverrideRulesConfig> {
        let rules = self
            .source
            .access
            .rule_selection(self.builder.context.file())
            .await?;
        let len = self.source.local(1, 0, || rules.iter().len()).await?;
        let work =
            SourceEffects::<A>::checked(len.checked_mul(40).and_then(|n| n.checked_add(40)))?;
        self.source
            .local(work, 0, || OverrideRulesConfig::from_rule_selection(rules))
            .await
    }

    async fn scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        let fields = self.source.access.endpoint().field_request_context();
        let scope = self
            .source
            .field(class.field_requests(fields).body_scope())
            .await?;
        let file = self.source.scope_file(scope).await?;
        self.source.check_file_program(file).await?;
        Ok(scope)
    }

    async fn own_members(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<FxHashSet<MemberWithDefinition<'db>>> {
        let mut cursor = self
            .source
            .scope_member_cursor(scope, self.builder.index)
            .await?;
        let mut members = self.source.local(4, 0, FxHashSet::default).await?;
        let mut name_bytes = 0usize;
        while let Some(member) = self.source.next_scope_member(&mut cursor).await? {
            let (len, capacity, name_len) = self
                .source
                .local(3, 0, || {
                    (members.len(), members.capacity(), member.member.name.len())
                })
                .await?;
            let (quote, next_slots) =
                storage::table_merge::<MemberWithDefinition<'db>>(len, capacity, 1, 0)
                    .ok_or(RunError::Contract("override member storage quote overflow"))?;
            let scan = SourceEffects::<A>::checked(
                next_slots.checked_mul(SourceEffects::<A>::checked(name_len.checked_add(4))?),
            )?;
            let work = SourceEffects::<A>::checked(
                quote
                    .work
                    .checked_add(scan)
                    .and_then(|n| n.checked_add(name_bytes))
                    .and_then(|n| n.checked_add(next_slots)),
            )?;
            // Hashing and collision equality inspect names. Existing names are also paid for if
            // reservation rehashes the table; the slot quote includes eventual table disposal.
            self.source
                .local(work, quote.bytes, || {
                    members.reserve(1);
                    #[cfg(test)]
                    let name = member.member.name.clone();
                    members.insert(member);
                    #[cfg(test)]
                    crate::types::infer::builder::source_definition::controlled::observations::override_member_retained(
                        self.builder.db(), &name, members.len(),
                    );
                })
                .await?;
            name_bytes = SourceEffects::<A>::checked(name_bytes.checked_add(name_len))?;
        }
        Ok(members)
    }

    async fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_identity_specialization_with(class, self).await
    }

    async fn inherited_conflicts(
        &self,
        class: StaticClassLiteral<'db>,
        _specialized: ClassType<'db>,
        _members: &FxHashSet<MemberWithDefinition<'db>>,
    ) -> RunResult<()> {
        if select_inherited_direct_bases_with(class, self)
            .await?
            .is_some()
        {
            return self
                .source
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
                .await;
        }
        Ok(())
    }

    async fn enum_metadata(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>> {
        self.source.access.enum_metadata(class).await
    }

    async fn mro_bases(&self, class: ClassType<'db>) -> RunResult<Vec<ClassBase<'db>>> {
        let fields = MroFieldReads::new(self.builder.db());
        let start = class_mro_start_with(fields, class, None, self.source).await?;
        let mut cursor = self
            .source
            .local(3, 0, || MroCursor::new(start.class, start.specialization))
            .await?;
        let mut bases = self.source.local(3, 0, Vec::new).await?;
        mro_next_with(fields, &mut cursor, MroDirection::Forward, self.source).await?;
        while let Some(base) =
            mro_next_with(fields, &mut cursor, MroDirection::Forward, self.source).await?
        {
            self.append_override_base(&mut bases, base).await?;
        }
        Ok(bases)
    }

    async fn new_generic_bases(&self) -> RunResult<GenericOverrideBases<'db>> {
        self.source.local(4, 0, GenericOverrideBases::default).await
    }

    async fn next_base(
        &self,
        bases: &[ClassBase<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.source
            .local(1, 0, || next_override_base(bases, cursor))
            .await
    }

    async fn generic_base(
        &self,
        base: ClassBase<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, GenericAlias<'db>)>> {
        let Some(base) = self
            .source
            .local(2, 0, || {
                base.into_class().and_then(ClassType::into_generic_alias)
            })
            .await?
        else {
            return Ok(None);
        };
        let fields = self.source.access.endpoint().field_request_context();
        let origin = self
            .source
            .field(base.field_requests(fields).origin())
            .await?;
        Ok(Some((origin, base)))
    }

    async fn insert_generic_base(
        &self,
        bases: &mut GenericOverrideBases<'db>,
        origin: StaticClassLiteral<'db>,
        base: GenericAlias<'db>,
    ) -> RunResult<()> {
        let (len, capacity) = self
            .source
            .local(2, 0, || (bases.len(), bases.capacity()))
            .await?;
        let (quote, slots) = storage::table_merge::<(StaticClassLiteral<'db>, GenericAlias<'db>)>(
            len, capacity, 1, 0,
        )
        .ok_or(RunError::Contract(
            "override generic base storage quote overflow",
        ))?;
        let work = SourceEffects::<A>::checked(
            slots.checked_mul(4).and_then(|n| n.checked_add(quote.work)),
        )?;
        self.source
            .local(work, quote.bytes, || {
                bases.reserve(1);
                bases.insert(origin, base);
            })
            .await
    }

    async fn augment_ancestors(
        &self,
        _class: ClassType<'db>,
        _bases: &mut Vec<ClassBase<'db>>,
        _generic: &GenericOverrideBases<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn member_cursor(
        &self,
        members: FxHashSet<MemberWithDefinition<'db>>,
    ) -> RunResult<IntoIter<MemberWithDefinition<'db>>> {
        let capacity = self.source.local(1, 0, || members.capacity()).await?;
        let work =
            SourceEffects::<A>::checked(storage::slots(capacity).and_then(|n| n.checked_add(3)))?;
        // Pay the backing scan before consuming the set; advancing the iterator can scan empty
        // buckets, while every member's destruction was paid before insertion.
        self.source.local(work, 0, || members.into_iter()).await
    }

    async fn next_member(
        &self,
        cursor: &mut IntoIter<MemberWithDefinition<'db>>,
    ) -> RunResult<Option<MemberWithDefinition<'db>>> {
        self.source.local(1, 0, || cursor.next()).await
    }

    async fn check_member(
        &self,
        configuration: OverrideRulesConfig,
        enum_info: Option<&'db EnumMetadata<'db>>,
        class: ClassType<'db>,
        scope: ScopeId<'db>,
        bases: &[ClassBase<'db>],
        member: &MemberWithDefinition<'db>,
    ) -> RunResult<()> {
        let request = self
            .source
            .initialize_value(|| OverrideMemberRequest {
                configuration,
                enum_info,
                class,
                scope,
                bases,
                member,
            })
            .await?;
        check_class_declaration_with(request, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OverrideMemberEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::instance(self.source, class).await
    }

    async fn lookup_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let member = lookup_override_member_with(class, name, OverrideLookupFacts, self).await?;
        #[cfg(test)]
        super::super::observations::observe(
            self.builder.db(),
            super::super::observations::Event::OverrideMemberLookupReady,
        );
        Ok(member)
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        self.source.work(1).await?;
        let identity = self.source.static_class_identity(class).await?;
        self.source
            .local(1, 0, || identity.map(|(literal, _)| literal))
            .await
    }

    async fn check_resolved_declaration(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        instance_of_class: Type<'db>,
        subclass_instance_member: PlaceAndQualifiers<'db>,
        type_on_subclass_instance: Type<'db>,
        literal: StaticClassLiteral<'db>,
    ) -> RunResult<()> {
        check_resolved_class_declaration_with(
            request,
            instance_of_class,
            subclass_instance_member,
            type_on_subclass_instance,
            literal,
            OverrideGeneratorFacts,
            self,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ResolvedOverrideMemberEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn code_generator(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        static_code_generator_with(literal, self.source).await
    }

    async fn generator_checkpoint(
        &self,
        work: OverrideGeneratorWork,
        name: &Name,
    ) -> RunResult<()> {
        let units = SourceEffects::<A>::checked(
            name.len()
                .checked_add(1)
                .and_then(|bytes| bytes.checked_mul(work.comparisons()))
                .and_then(|units| units.checked_add(3)),
        )?;
        self.source.work(units).await
    }

    async fn named_tuple_attribute(
        &self,
        _request: OverrideMemberRequest<'_, 'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::NamedTuple))
            .await
    }

    async fn post_init_signature(
        &self,
        _request: OverrideMemberRequest<'_, 'db>,
        _policy: CodeGeneratorKind<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::DataclassFields))
            .await
    }

    async fn remaining_declaration(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        instance_of_class: Type<'db>,
        subclass_instance_member: PlaceAndQualifiers<'db>,
        type_on_subclass_instance: Type<'db>,
        literal: StaticClassLiteral<'db>,
        class_kind: Option<CodeGeneratorKind<'db>>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| crate::types::overrides::remaining::check_remaining_with(
                request,
                instance_of_class,
                subclass_instance_member,
                type_on_subclass_instance,
                literal,
                class_kind,
                self,
            ))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OverrideLookupEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &Name) -> RunResult<()> {
        self.source
            .work(SourceEffects::<A>::checked(name.len().checked_add(1))?)
            .await
    }

    async fn instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::instance(self.source, class).await
    }

    async fn class_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source
            .source_class_mro(class, name, MemberLookupPolicy::default())
            .await
    }

    async fn instance_member(
        &self,
        instance: Type<'db>,
        name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let result = self
            .source
            .allocate_future(|| {
                member_lookup_entry_with(
                    instance,
                    GeneralMemberName::Shared(name),
                    MemberLookupPolicy::default(),
                    None,
                    GeneralMemberFacts,
                    self.source,
                )
            })
            .await?
            .await?;
        self.source.work(1).await?;
        let member = match result {
            Ok(member) => member,
            Err(error) => {
                self.source
                    .field(
                        error
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .fallback_member(),
                    )
                    .await?
            }
        };
        self.source.work(1).await?;
        match member {
            ResolvedMember::Plain(member) => Ok(member),
            ResolvedMember::WithMetadata(metadata) => {
                self.source
                    .field(
                        metadata
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .member(),
                    )
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassIdentityEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.source.access.class_generic_context(class).await
    }

    async fn identity_alias(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.source
            .local_quoted_with_fixed_transfers(
                Ok((
                    CALL_3 + 4,
                    size_of::<[
                        (
                            &SourceEffects<'_, 'run, 'db, A>,
                            StaticClassLiteral<'db>,
                            GenericContext<'db>,
                        );
                        2
                    ]>() + size_of::<[RunResult<ClassType<'db>>; 4]>(),
                )),
                || ClassIdentityEffects::identity_alias(self.source, class, context),
            )
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InheritedBaseSelectionEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn empty_direct_bases(&self) -> RunResult<Vec<ClassType<'db>>> {
        self.source.local(3, 0, Vec::new).await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self.source).await
    }

    async fn next_explicit_base(
        &self,
        bases: &[Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(1, 0, || next_inherited_explicit_base(bases, cursor))
            .await
    }

    async fn classify_explicit_base(
        &self,
        _class: StaticClassLiteral<'db>,
        base: Type<'db>,
    ) -> RunResult<InheritedBaseSelection<'db>> {
        let conversion = self
            .source
            .local(1, 0, || ClassBaseConversion::from_explicit_type(base))
            .await?;
        let base = match conversion {
            ClassBaseConversion::Ready(base) => base,
            ClassBaseConversion::Dependency(ClassBaseDependency::DefaultSpecialization(
                ClassLiteral::Static(class),
            )) => Some(ClassBase::Class(
                class_default_specialization_with(class, self.source).await?,
            )),
            ClassBaseConversion::Dependency(_) => {
                return self
                    .source
                    .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
                    .await;
            }
        };
        self.source.work(3).await?;
        classify_inherited_base_with(base, self).await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.source.work(1).await?;
        self.source.static_class_identity(class).await
    }

    async fn append_direct_base(
        &self,
        bases: &mut Vec<ClassType<'db>>,
        base: ClassType<'db>,
    ) -> RunResult<()> {
        self.append_override_base(bases, base).await
    }

    async fn has_multiple_direct_bases(&self, bases: &[ClassType<'db>]) -> RunResult<bool> {
        self.source.local(1, 0, || bases.len() >= 2).await
    }

    async fn mro_is_error(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let mro = self.source.access.static_mro(class).await?;
        self.source.local(1, 0, || mro.is_err()).await
    }
}
