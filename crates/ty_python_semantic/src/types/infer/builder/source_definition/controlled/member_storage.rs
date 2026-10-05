use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::place::{PlaceAndQualifiers, Provenance};
use crate::types::class::implicit_attributes::{
    AugmentedBindings, ImplicitAttribute, implicit_attribute_bindings_with,
};
use crate::types::class::instance_storage::{
    ClassInstanceStorageEffects, InstanceStorageWork, StaticInstanceStorageEffects,
    class_instance_member_with, class_own_instance_member_with, static_instance_member_with,
    static_is_typed_dict_with,
};
use crate::types::class::member_lookup::{
    self, InstanceMroEffects, InstanceMroWork, MemberFinalizationEffects, MemberFinalizationWork,
    MroPendingBindings, mro_instance_member_with,
};
use crate::types::class::member_source::static_own_instance_member_with;
use crate::types::class::slots::lacks_instance_storage_with;
use crate::types::class::{
    DynamicClassLiteral, DynamicEnumLiteral, DynamicNamedTupleLiteral, InstanceMemberResult,
    MethodDecorator,
};
use crate::types::class_base::ClassBase;
use crate::types::descriptor::effects::DescriptorOperation;
use crate::types::member::Member;
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::storage_quote::{buffer_push_quote, buffer_retirement};
use crate::types::{
    ClassType, GenericAlias, Specialization, StaticClassLiteral, Type, UnionBuilder,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn class_instance_storage(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.allocate_future(|| class_instance_member_with(&env, class, name, self))
            .await?
            .await
    }

    pub(super) async fn push_member_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local(1, 0, || {
                buffer_push_quote::<(ClassType<'db>, AugmentedBindings<'db>)>((
                    pending.len(),
                    pending.capacity(),
                    pending.capacity() != 0,
                ))
            })
            .await?
            .ok_or(RunError::Contract(
                "member pending buffer quotation overflow",
            ))?;
        self.local(quote.work, quote.bytes, || pending.push((class, bindings)))
            .await
    }

    pub(super) async fn clear_member_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        let work = self
            .local(1, 0, || {
                buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
                    pending.len(),
                    0,
                    false,
                ))
            })
            .await?
            .ok_or(RunError::Contract(
                "member pending buffer quotation overflow",
            ))?;
        self.local(work, 0, || pending.clear()).await
    }

    pub(super) async fn finish_member_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        let work = self
            .local(1, 0, || {
                buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
                    pending.len(),
                    pending.capacity(),
                    pending.capacity() != 0,
                ))
            })
            .await?
            .ok_or(RunError::Contract(
                "member pending buffer quotation overflow",
            ))?;
        self.work(work).await?;
        drop(pending);
        Ok(())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassInstanceStorageEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: InstanceStorageWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        self.field(alias.field_requests(self.db()).origin()).await
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.field(alias.field_requests(self.db()).specialization())
            .await
    }

    async fn dynamic_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn named_tuple_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn enum_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn dynamic_own_instance_member(
        &self,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn named_tuple_own_instance_member(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn enum_own_instance_member(
        &self,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_typed_dict_with(class, self).await
    }

    async fn static_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.environment_program(env).await?;
        self.check_file_program(self.static_class_file(class).await?)
            .await?;
        self.allocate_future(|| static_instance_member_with(env, class, specialization, name, self))
            .await?
            .await
    }

    async fn static_own_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> RunResult<Member<'db>> {
        self.environment_program(env).await?;
        self.allocate_future(|| static_own_instance_member_with(env, class, name, self))
            .await?
            .await
    }

    async fn specialize_place(
        &self,
        member: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let ty = self.local(1, 0, || member.place.raw_type()).await?;
        let (Some(ty), Some(specialization)) = (ty, specialization) else {
            return Ok(member);
        };
        let specialized = self
            .access
            .apply_specialization(ty, specialization, true)
            .await?;
        self.local(size_of::<PlaceAndQualifiers<'db>>() * 2 + 1, 0, || {
            member.map_type(|_| specialized)
        })
        .await
    }

    async fn specialize_member(
        &self,
        member: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<Member<'db>> {
        let inner =
            ClassInstanceStorageEffects::specialize_place(self, member.inner, specialization)
                .await?;
        self.local(size_of::<Member<'db>>() + 1, 0, || Member { inner })
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticInstanceStorageEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn storage_checkpoint(&self, _work: InstanceStorageWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_typed_dict_with(class, self).await
    }

    async fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> RunResult<bool> {
        self.allocate_future(|| lacks_instance_storage_with(class, name, self))
            .await?
            .await
    }

    async fn mro_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> RunResult<InstanceMemberResult<'db>> {
        self.environment_program(env).await?;
        let cursor = self
            .local(size_of::<MroCursor<'db>>() * 2 + 1, 0, || {
                MroCursor::new(class.into(), specialization)
            })
            .await?;
        self.allocate_future(|| mro_instance_member_with(name, cursor, self))
            .await?
            .await
    }

    async fn typed_dict_fallback(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> member_lookup::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InstanceMroEffects<'db, MroCursor<'db>>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: InstanceMroWork) -> RunResult<()> {
        let extra = match work {
            InstanceMroWork::PushAugmented { prefix_len } => prefix_len,
            InstanceMroWork::InferAugmented { pending_len }
            | InstanceMroWork::ClearAugmented { pending_len } => pending_len,
            _ => 0,
        };
        self.work(Self::checked(extra.checked_add(1))?).await
    }

    async fn new_union(&self) -> RunResult<UnionBuilder<'db>> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        PairUnionEffects::new_union(self, &env).await
    }

    async fn advance(&self, cursor: &mut MroCursor<'db>) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<Member<'db>> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.allocate_future(|| class_own_instance_member_with(&env, class, name, self))
            .await?
            .await
    }

    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<Option<ImplicitAttribute<'db>>> {
        self.work(1).await?;
        let Some((class, _)) = self.static_class_identity(class).await? else {
            return Ok(None);
        };
        self.allocate_future(|| {
            implicit_attribute_bindings_with(class, name, MethodDecorator::None, self)
        })
        .await?
        .await
        .map(Some)
    }

    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> RunResult<()> {
        self.push_member_pending(pending, class, bindings).await
    }

    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        self.clear_member_pending(pending).await
    }

    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        self.finish_member_pending(pending).await
    }

    async fn infer_augmented(
        &self,
        _bindings: MroPendingBindings<'_, 'db>,
    ) -> RunResult<(Type<'db>, Provenance<'db>)> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn union_add(
        &self,
        mut union: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        PairUnionEffects::union_add(self, &mut union, ty).await?;
        Ok(union)
    }

    async fn own_class_member(&self, class: ClassType<'db>, name: &str) -> RunResult<Member<'db>> {
        self.source_own_class_member(class, name, None).await
    }

    async fn is_definitely_non_data_descriptor(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::DataDescriptor,
        ))
        .await
    }

    async fn union_build(&self, union: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::union_build(self, union).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberFinalizationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: MemberFinalizationWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn intersect_dynamic(&self, ty: Type<'db>, dynamic: Type<'db>) -> RunResult<Type<'db>> {
        self.access
            .intersection_from_two_elements(ty, dynamic)
            .await
    }
}
