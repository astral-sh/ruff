use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::FixedFieldCopy;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class::instance_flags::{
    ExplicitAnyInheritanceEffects, InstanceFlagFacts, InstanceFlagsWork, QueuedInstanceFlags,
    queued_inherited_flags_with, queued_instance_flags_with, queued_own_getattribute_with,
};
use crate::types::class::instance_storage::inherits_from_explicit_any_without_inference_with;
use crate::types::class::{ClassInstanceFlags, KnownClassInstanceOperation};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, KnownClass, Specialization, StaticClassLiteral,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        queued_inherited_flags_with(class, InstanceFlagFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> QueuedInstanceFlags<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Cursor = MroCursor<'db>;

    async fn checkpoint(&self, _work: InstanceFlagsWork) -> RunResult<()> {
        self.work(4).await
    }

    async fn known_class(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .known(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .has_explicit_bases(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .has_explicit_metaclass(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn inherited_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        self.access.inherited_instance_flags(class).await
    }

    async fn own_attribute(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        queued_own_getattribute_with(class, self).await
    }

    async fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let scope = self
            .field_with_profile(
                class
                    .field_requests(self.access.endpoint().field_request_context())
                    .body_scope(),
                &FixedFieldCopy,
            )
            .await?;
        let table = self.access.place_table(scope).await?;
        let work = Self::checked(table.symbol_lookup_work("__getattribute__".len()))?;
        self.local(work, size_of::<bool>(), || {
            table.symbol_id("__getattribute__").is_some()
        })
        .await
    }

    async fn metaclass_custom_getattribute(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::InstanceFlagsMetaclass)
            .await
    }

    async fn start_mro(&self, class: StaticClassLiteral<'db>) -> RunResult<Self::Cursor> {
        self.local(2, size_of::<MroCursor<'db>>() * 2, || {
            MroCursor::new(class.into(), None)
        })
        .await
    }

    async fn next_base(&self, cursor: &mut Self::Cursor) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.work(1).await?;
        self.static_class_identity(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExplicitAnyInheritanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn without_inference(&self, class: ClassLiteral<'db>) -> RunResult<Option<bool>> {
        inherits_from_explicit_any_without_inference_with(class, self).await
    }

    async fn instance_flags(&self, class: ClassLiteral<'db>) -> RunResult<ClassInstanceFlags> {
        let ClassLiteral::Static(class) = class else {
            return self
                .unavailable(SourceOperation::KnownClassInstance(
                    KnownClassInstanceOperation::ExplicitAnyInheritance,
                ))
                .await;
        };
        queued_instance_flags_with(class, InstanceFlagFacts, self).await
    }
}
