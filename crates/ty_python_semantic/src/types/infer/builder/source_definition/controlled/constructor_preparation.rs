//! Constructor metadata uses canonical queries and the invocation's existing callable guard.

use salsa::execution_probe::{RunError, RunResult};

use super::callable_guard::GuardedPreparationEffects;
use super::class_selection::FixedFieldCopy;
use super::{SourceAccess, SourceOperation};
use crate::analysis::ConstructorPreparationOperation;
use crate::types::class::CodeGeneratorKind;
use crate::types::class::KnownClassInstanceEffects;
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::constructor::bindings::ConstructorBindingsEffects;
use crate::types::constructor::member_resolution::ObjectInitializer;
use crate::types::constructor::{ConstructorMember, ConstructorMembers};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::cyclic::entry::CallableDefinitionEffects;
use crate::types::member_lookup::mro_dispatch::MroLookupEffects;
use crate::types::relation::source::resources::{ClassRelation, RelationResourceAccess};
use crate::types::{ClassLiteral, ClassType, GenericContext, KnownClass, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorBindingsEffects<'db>
    for GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    async fn decision<T: Copy>(&self, action: impl FnOnce() -> T) -> RunResult<T> {
        let bytes = size_of::<T>().checked_mul(2).ok_or(RunError::Contract(
            "constructor decision transfer quotation overflow",
        ))?;
        self.source.local(1, bytes, action).await
    }

    async fn class_literal(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<ClassLiteral<'db>> {
        match class {
            ClassType::NonGeneric(literal) => self.decision(|| literal).await,
            ClassType::Generic(alias) => {
                let origin = self
                    .source
                    .field_with_profile(
                        alias
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .origin(),
                        &FixedFieldCopy,
                    )
                    .await?;
                self.decision(|| ClassLiteral::Static(origin)).await
            }
        }
    }

    async fn generic_context(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        match class {
            ClassLiteral::Static(class) => self.source.access.class_generic_context(class).await,
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => self.decision(|| None).await,
        }
    }

    async fn is_typed_dict(&self, _db: &'db dyn Db, class: ClassLiteral<'db>) -> RunResult<bool> {
        let class = self.decision(|| ClassType::NonGeneric(class)).await?;
        MroLookupEffects::is_typed_dict(self.source, class).await
    }

    async fn generated_typed_dict(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        let generator = match class {
            ClassLiteral::Static(class) => self.source.access.code_generator(class).await?,
            ClassLiteral::Dynamic(_) => {
                return self
                    .source
                    .unavailable(SourceOperation::ConstructorPreparation(
                        ConstructorPreparationOperation::DynamicCodeGenerator,
                    ))
                    .await;
            }
            ClassLiteral::DynamicNamedTuple(_) => {
                self.decision(|| Some(CodeGeneratorKind::NamedTuple))
                    .await?
            }
            ClassLiteral::DynamicTypedDict(_) => {
                self.decision(|| Some(CodeGeneratorKind::TypedDict)).await?
            }
            ClassLiteral::DynamicEnum(_) => self.decision(|| None).await?,
        };
        self.decision(|| match generator {
            Some(CodeGeneratorKind::TypedDict) => true,
            Some(
                CodeGeneratorKind::DataclassLike(_)
                | CodeGeneratorKind::Pydantic(_)
                | CodeGeneratorKind::NamedTuple,
            )
            | None => false,
        })
        .await
    }

    async fn known(&self, db: &'db dyn Db, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        let class = self.class_literal(db, class).await?;
        match class {
            ClassLiteral::Static(class) => {
                self.source
                    .field_with_profile(
                        class
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .known(),
                        &FixedFieldCopy,
                    )
                    .await
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => self.decision(|| None).await,
        }
    }

    async fn enum_class(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        let literal =
            KnownClassInstanceEffects::class_literal(self.source, KnownClass::Enum).await?;
        KnownClassInstanceEffects::to_class_type(self.source, literal).await
    }

    async fn is_subclass(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<bool> {
        let bytes = size_of::<ClassType<'db>>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<ClassRelation>()))
            .and_then(|bytes| bytes.checked_add(size_of::<A::Resources>()))
            .ok_or(RunError::Contract(
                "constructor class relation quotation overflow",
            ))?;
        let resources = self
            .source
            .local(4, bytes, || self.source.access.resources())
            .await?;
        resources
            .class_condition(
                db,
                env,
                class,
                target,
                ClassRelation::Subtyping,
                self.source,
            )
            .await
    }

    async fn identity_class(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        CallableDefinitionEffects::identity_class(self.source, class).await
    }

    async fn instance_approximation(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        NamespaceLookupEffects::instance_approximation(self.source, receiver).await
    }

    async fn to_class_type(
        &self,
        _db: &'db dyn Db,
        receiver: Type<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        KnownClassInstanceEffects::to_class_type(self.source, receiver).await
    }

    async fn metaclass_call(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        self.source.constructor_metaclass_call(members, guard).await
    }

    async fn new_method(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        self.source.constructor_new_method(members, guard).await
    }

    async fn initializer(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        members: ConstructorMembers<'db>,
        include_object: ObjectInitializer,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        self.source
            .constructor_initializer(members, include_object, guard)
            .await
    }
}
