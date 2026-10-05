//! Known-class lookup through the canonical module and definition producers.

use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::KnownModule;

use super::class_selection::FixedFieldCopy;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::{Place, known_module_symbol_with};
use crate::types::class::instance_flags::{
    InstanceFlagFacts, inherits_from_explicit_any_with, queued_instance_flags_with,
};
use crate::types::class::instance_storage::{
    InstanceClassificationEffects, InstanceStorageWork, sealed as instance_storage_sealed,
    static_is_typed_dict_with,
};
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::class::type_conversion::{TypeToClassEffects, type_to_class_type_with};
use crate::types::class::{
    ClassInstanceFlags, KnownClass, KnownClassInstanceEffects, KnownClassInstanceOperation,
    KnownClassLookupEffects, KnownClassLookupError, KnownClassLookupFacts, StaticClassLiteral,
    class_default_specialization_with, interpret_class_literal_lookup,
    known_class_to_class_literal_with, known_class_to_instance_with,
};
use crate::types::instance::effects::{InstanceEffects, InstanceWork};
use crate::types::signatures::effects::sealed as instance_sealed;
use crate::types::tuple::TupleType;
use crate::types::{ClassLiteral, ClassType, Specialization, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_known_class(
        &self,
        class: KnownClass,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>> {
        let resolver = self
            .field(
                self.program
                    .field_requests(self.db())
                    .resolver_environment(),
            )
            .await?;
        let version = self
            .field(resolver.read_fields(self.db()).python_version())
            .await?;
        known_class_to_class_literal_with(class, version, KnownClassLookupFacts, self).await
    }

    pub(in crate::types::infer) async fn infer_known_class_instance(
        &self,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        known_class_to_instance_with(class, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn known_module_symbol(&self, module: KnownModule, name: &str) -> RunResult<Place<'db>> {
        let env = self
            .initialize_value(|| ProgramEnvironment::from_program(self.program))
            .await?;
        Ok(
            known_module_symbol_with(self.db(), &env, self, module, name)
                .await?
                .place,
        )
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassInstanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn class_literal(&self, class: KnownClass) -> RunResult<Type<'db>> {
        let result = self.access.known_class_lookup(self.program, class).await?;
        self.local(2, size_of::<Type<'db>>(), || {
            interpret_class_literal_lookup(result)
                .map(|class| Type::ClassLiteral(ClassLiteral::Static(class)))
                .unwrap_or_else(Type::unknown)
        })
        .await
    }

    async fn to_class_type(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        type_to_class_type_with(ty, self).await
    }

    async fn instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        let env = self
            .initialize_value(|| ProgramEnvironment::from_program(self.program))
            .await?;
        Type::instance_with(self.db(), &env, self, class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeToClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn default_specialization(&self, class: ClassLiteral<'db>) -> RunResult<ClassType<'db>> {
        let ClassLiteral::Static(class) = class else {
            return self
                .unavailable(SourceOperation::KnownClassInstance(
                    KnownClassInstanceOperation::DefaultSpecialization,
                ))
                .await;
        };
        class_default_specialization_with(class, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> instance_sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InstanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: InstanceWork) -> RunResult<()> {
        let bytes = match work {
            InstanceWork::Dispatch => 0,
            InstanceWork::Publish => size_of::<Type<'db>>(),
        };
        self.local(8, bytes, || ()).await
    }

    async fn class_literal_and_specialization(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<(ClassLiteral<'db>, Option<Specialization<'db>>)> {
        match class {
            ClassType::NonGeneric(literal) => self.initialize_value(|| (literal, None)).await,
            ClassType::Generic(alias) => {
                let fields = self.access.endpoint().field_request_context();
                let origin = self
                    .field_with_profile(alias.field_requests(fields).origin(), &FixedFieldCopy)
                    .await?;
                let specialization = self
                    .field_with_profile(
                        alias.field_requests(fields).specialization(),
                        &FixedFieldCopy,
                    )
                    .await?;
                self.initialize_value(|| (ClassLiteral::Static(origin), Some(specialization)))
                    .await
            }
        }
    }

    async fn known_class(
        &self,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .known(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn is_typed_dict(
        &self,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.local(1, size_of::<bool>(), || ()).await?;
        static_is_typed_dict_with(class, self).await
    }

    async fn is_protocol(
        &self,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.local(1, size_of::<bool>(), || ()).await?;
        static_is_protocol_with(class, self).await
    }

    async fn inherits_from_explicit_any(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.local(1, size_of::<bool>(), || ()).await?;
        if inherits_from_explicit_any_with(class, InstanceFlagFacts, self).await? {
            return self
                .unavailable(SourceOperation::ExplicitAnyInstanceConstruction)
                .await;
        }
        self.initialize_value(|| false).await
    }

    async fn tuple(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> RunResult<TupleType<'db>> {
        self.unavailable(SourceOperation::KnownClassInstance(
            KnownClassInstanceOperation::TupleNormalization,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> instance_storage_sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InstanceClassificationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.field_with_profile(class.field_requests(self.db()).known(), &FixedFieldCopy)
            .await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field_with_profile(
            class.field_requests(self.db()).has_explicit_bases(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn checkpoint(&self, _work: InstanceStorageWork) -> RunResult<()> {
        self.work(3).await
    }

    async fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        queued_instance_flags_with(class, InstanceFlagFacts, self).await
    }
}
