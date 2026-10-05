//! Class selection uses the ordinary dispatch with explicit semantic dependencies.

use salsa::execution_probe::{
    ExecutionWork, FieldReadProfile, FieldReturnMode, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class::KnownClassInstanceEffects;
use crate::types::class_selection::{self, LiteralMetaTypeEffects, NominalSelectionEffects};
use crate::types::local_transfer::generated_field_quote;
use crate::types::instance::{
    ExplicitAnyInstanceClass, NominalClassEffects, NominalClassFacts, NominalGenericEffects,
    NominalInstanceClass, NominalKnownClassEffects, nominal_class_with,
};
use crate::types::literal::{EnumLiteralType, LiteralFallback};
use crate::types::newtype::NewType;
use crate::types::tuple::TupleType;
use crate::types::type_alias::AliasResolutionStep;
use crate::types::typevar::TypeVarBoundOrConstraints;
use crate::types::{
    BoundTypeVarInstance, ClassType, GenericAlias, KnownClass, LiteralValueType,
    NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, Specialization,
    StaticClassLiteral, Type, TypedDictType,
};

/// Quotes fixed field copies with separate work and representation-byte charges.
#[derive(Debug)]
pub(in crate::types::infer) struct FixedFieldCopy;

impl<T: Copy> FieldReadProfile<T> for FixedFieldCopy {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call T,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<NativeValueQuote>()
                        + size_of::<RunResult<NativeValueQuote>>(),
                })?;
                endpoint.check_completion()?;
                if mode != FieldReturnMode::Copy {
                    return Err(RunError::Contract("fixed field profile requires copy conversion"));
                }
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<T>(),
                    cleanup_work: 0,
                })
            })
            .await)
    }
}

/// Quotes a borrowed field handle without visiting or copying its retained payload.
#[derive(Debug)]
pub(in crate::types::infer) struct FixedFieldBorrow;

impl<T> FieldReadProfile<T> for FixedFieldBorrow {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call T,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<NativeValueQuote>()
                        + size_of::<RunResult<NativeValueQuote>>(),
                })?;
                endpoint.check_completion()?;
                if mode != FieldReturnMode::Ref {
                    return Err(RunError::Contract("fixed field profile requires borrowed conversion"));
                }
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<&T>(),
                    cleanup_work: 0,
                })
            })
            .await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LiteralMetaTypeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Reserve the shared selector's fixed target before it is constructed.
        self.local(1, size_of::<LiteralFallback<'db>>(), || ()).await
    }

    async fn known_class_literal(&self, class: KnownClass) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::class_literal(self, class).await
    }

    async fn enum_class_literal(&self, literal: EnumLiteralType<'db>) -> RunResult<Type<'db>> {
        let enum_class = self
            .field_with_profile(literal.field_requests(self.db()).enum_class_literal(), &FixedFieldCopy)
            .await?;
        let class = self
            .field_with_profile(enum_class.field_requests(self.db()).class_literal(), &FixedFieldCopy)
            .await?;
        self.initialize_value(|| Type::ClassLiteral(class)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NominalSelectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local_with_fixed_transfers(16, size_of::<Type<'db>>() * 4 + size_of::<ClassType<'db>>() * 4, || ()).await
    }

    async fn next_type(&self, current: &mut Option<Type<'db>>) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(1, 0, || current.take()).await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        match self.local_with_fixed_transfers(2, 0, || ty.alias_resolution_step()).await? {
            AliasResolutionStep::Resolved(ty) => Ok(ty),
            AliasResolutionStep::Alias(_)
            | AliasResolutionStep::Recursive(_)
            | AliasResolutionStep::UnboundRecursiveVariable => {
                self.unavailable(SourceOperation::TypeAliasResolution).await
            }
        }
    }

    async fn nominal_class(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        class_selection::nominal_class_with(ty, self).await
    }

    async fn typed_dict_class(
        &self,
        typed_dict: TypedDictType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.local_with_fixed_transfers(1, 0, || typed_dict.defining_class()).await
    }

    async fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        nominal_class_with(instance, NominalClassFacts, self).await
    }

    async fn protocol_class(
        &self,
        _instance: ProtocolInstanceType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn newtype_base(&self, _newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn typevar_bound(
        &self,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn literal_fallback(&self, _literal: LiteralValueType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn property_fallback(
        &self,
        _property: PropertyInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn known_instance(&self, class: KnownClass) -> RunResult<Type<'db>> {
        self.access.known_class_instance(self.program, class).await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.local_with_fixed_transfers(16, size_of::<Type<'db>>() * 4 + size_of::<ClassType<'db>>() * 4, || ()).await?;
        self.static_class_identity(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NominalClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local_with_fixed_transfers(16, size_of::<Type<'db>>() * 4 + size_of::<ClassType<'db>>() * 4, || ()).await
    }

    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        match class {
            NominalInstanceClass::Plain(class) => self.local_with_fixed_transfers(1, 0, || class).await,
            NominalInstanceClass::InheritsFromExplicitAny(class) => {
                let request = self.local_with_fixed_transfers(16, 0, || {
                    class.field_requests(self.access.endpoint().field_request_context()).class()
                }).await?;
                self.field(request).await
            }
        }
    }

    async fn tuple_class(&self, tuple: TupleType<'db>) -> RunResult<ClassType<'db>> {
        self.access.tuple_class(tuple).await
    }

    async fn version_class(&self) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable(SourceOperation::ClassSelection).await
    }

    async fn object_class(&self) -> RunResult<ClassType<'db>> {
        let ty = KnownClassInstanceEffects::class_literal(self, KnownClass::Object).await?;
        let class = self
            .local_with_fixed_transfers(2, 0, || ty.as_class_literal().map(ClassType::NonGeneric))
            .await?;
        match class {
            Some(class) => Ok(class),
            None => self.unavailable(SourceOperation::ClassSelection).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NominalKnownClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(
            const { Ok((5 + 19, (5 + 19) * size_of::<(NominalInstanceType<'db>, ClassType<'db>, Option<KnownClass>)>())) },
            || (),
        ).await
    }

    async fn explicit_any_class(&self, class: ExplicitAnyInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        Ok(self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |class: ExplicitAnyInstanceClass<'db>, context| class.field_requests(context),
                |class: ExplicitAnyInstanceClass<'db>, context| class.field_requests(context).class(),
            ),
            || self.access.endpoint().read_field(class.field_requests(self.access.endpoint().field_request_context()).class(), &FixedFieldCopy),
        ).await?.await)
    }

    async fn generic_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        Ok(self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |alias: GenericAlias<'db>, context| alias.field_requests(context),
                |alias: GenericAlias<'db>, context| alias.field_requests(context).origin(),
            ),
            || self.access.endpoint().read_field(alias.field_requests(self.access.endpoint().field_request_context()).origin(), &FixedFieldCopy),
        ).await?.await)
    }

    async fn static_known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        Ok(self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |class: StaticClassLiteral<'db>, context| class.field_requests(context),
                |class: StaticClassLiteral<'db>, context| class.field_requests(context).known(),
            ),
            || self.access.endpoint().read_field(class.field_requests(self.access.endpoint().field_request_context()).known(), &FixedFieldCopy),
        ).await?.await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NominalGenericEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn non_tuple_is_generic(&self, class: NominalInstanceClass<'db>) -> RunResult<bool> {
        let class = NominalClassEffects::non_tuple_class(self, class).await?;
        self.local(1, 0, || class.is_generic()).await
    }
}
