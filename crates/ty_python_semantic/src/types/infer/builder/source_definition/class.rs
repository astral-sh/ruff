//! Class-definition dependencies admitted by an owned source transaction.
//!
//! Scope and known-type identification read structural source metadata. Decorator inference,
//! expression inference, and type relations require separate admitted operations before the
//! shared class body can use them.

use std::future::{Future, ready};

use ruff_python_ast as ast;
use salsa::execution_probe::FieldRequest;
use ty_module_resolver::{ImportingFile, KnownModule, file_to_module};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::types::call::CallError;
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::context::InferContext;
use crate::types::function::{DataclassTransformerParams, FunctionType};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::class::source_effects::{
    ClassDefinitionEffects, ClassDefinitionWork, ClassIdentity, LegacyInlineEffects, sealed,
};
use crate::types::signatures::effects::legacy_inline;
use crate::types::{DataclassParams, KnownClass, StaticClassLiteral, Type, TypeContext};
use crate::{Db, ProgramEnvironment};

impl sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> ClassDefinitionEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error> {
        Ok(context.in_stub())
    }

    async fn checkpoint(&self, work: ClassDefinitionWork) -> Result<(), Self::Error> {
        let units = match work {
            ClassDefinitionWork::InspectDefinition {
                decorators,
                keywords,
                name_bytes,
            } => {
                // Account for two decorator passes, two keyword scans, and the class name.
                decorators
                    .checked_mul(2)
                    .and_then(|count| {
                        keywords
                            .checked_mul(2)
                            .and_then(|keywords| count.checked_add(keywords))
                    })
                    .and_then(|count| count.checked_add(name_bytes))
                    .and_then(|count| count.checked_add(1))
                    .ok_or(Boundary::CostOverflow)?
            }
            ClassDefinitionWork::DecoratorExpression
            | ClassDefinitionWork::MetadataDecorator
            | ClassDefinitionWork::RuntimeDecorator
            | ClassDefinitionWork::OriginalClass
            | ClassDefinitionWork::RecordBinding
            | ClassDefinitionWork::KeywordExpression
            | ClassDefinitionWork::BaseExpression
            | ClassDefinitionWork::RecordDeferred => 1,
        };
        self.work(units).await
    }

    async fn allocate_vec<T>(&self, capacity: usize) -> Result<Vec<T>, Self::Error> {
        self.work(capacity.checked_add(1).ok_or(Boundary::CostOverflow)?)
            .await?;
        Ok(legacy_inline(LegacyInlineEffects.allocate_vec(capacity)))
    }

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        identity: ClassIdentity<'_, 'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        self.work(
            identity
                .name
                .len()
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?,
        )
        .await?;
        Ok(legacy_inline(
            LegacyInlineEffects.class_literal(db, identity),
        ))
    }

    async fn bind_class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
        ty: Type<'db>,
        undecorated_ty: Option<Type<'db>>,
    ) -> Result<(), Self::Error> {
        self.work(2).await?;
        legacy_inline(LegacyInlineEffects.bind_class(
            builder,
            class,
            definition,
            ty,
            undecorated_ty,
        ));
        Ok(())
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.work(1).await?;
        legacy_inline(LegacyInlineEffects.record_deferred(builder, definition));
        Ok(())
    }

    async fn infer_class_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error> {
        builder
            .infer_expression_with(self, &decorator.expression, TypeContext::default())
            .await
    }

    fn infer_class_expression(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassExpression)))
    }

    async fn class_body_scope(
        &self,
        db: &'db dyn Db,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
    ) -> Result<ScopeId<'db>, Self::Error> {
        let file = builder.program_file();
        if file.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(scope.to_scope_id(db, file))
    }

    async fn known_class(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
        name: &str,
    ) -> Result<Option<KnownClass>, Self::Error> {
        self.work(name.len().checked_add(1).ok_or(Boundary::CostOverflow)?)
            .await?;
        let importing_file = ImportingFile::File(
            context.file(),
            context.program_environment().resolver_environment(db),
        );
        Ok(KnownClass::try_from_file_and_name(db, importing_file, name))
    }

    async fn known_module(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
    ) -> Result<Option<KnownModule>, Self::Error> {
        self.work(1).await?;
        let importing_file = ImportingFile::File(
            context.file(),
            context.program_environment().resolver_environment(db),
        );
        Ok(
            file_to_module(db, importing_file.resolver_file(db))
                .and_then(|module| module.known(db)),
        )
    }

    fn default_dataclass_params(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<DataclassParams<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassMetadata)))
    }

    fn dataclass_transformer_params(
        &self,
        _db: &'db dyn Db,
        _function: FunctionType<'db>,
    ) -> impl Future<Output = Result<Option<DataclassTransformerParams<'db>>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassMetadata)))
    }

    fn dataclass_params_from_transformer(
        &self,
        _db: &'db dyn Db,
        _params: DataclassTransformerParams<'db>,
    ) -> impl Future<Output = Result<DataclassParams<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassMetadata)))
    }

    fn apply_class_decorator(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _decorator_ty: Type<'db>,
        _decorated_ty: Type<'db>,
    ) -> impl Future<Output = Result<Result<Type<'db>, CallError<'db>>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassDecorator)))
    }

    fn decorator_error_return_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _error: &CallError<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassDecorator)))
    }

    fn is_unknown_decorator_result(
        &self,
        _db: &'db dyn Db,
        _result_ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassRelation)))
    }

    fn type_retains_original_class(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _decorated_class: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassRelation)))
    }

    fn class_decorator_preserves_class_binding(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _decorated_class: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassRelation)))
    }

    fn merge_class_preserving_decorator_result(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _original_class: Type<'db>,
        _current_binding: Type<'db>,
        _decorated_binding: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ClassRelation)))
    }

    async fn class_bases_contain_string_literal(
        &self,
        class: &ast::StmtClassDef,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        if class.bases().is_empty() {
            Ok(false)
        } else {
            Err(unsupported(SourceDefinitionEffect::ClassExpression))
        }
    }
}
