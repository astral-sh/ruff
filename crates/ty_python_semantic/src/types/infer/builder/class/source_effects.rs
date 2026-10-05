//! Dependencies of class-definition inference that may require another source fact.
//!
//! The shared class body owns decorator ordering. Providers complete dependencies, allocate
//! decorator storage, and write local results before that body continues. The enclosing definition
//! transaction must discard its builder on error or cancellation; local writes are not completed
//! facts. Only the immediate provider invokes the synchronous semantic helpers below.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use salsa::execution_probe::FieldRequest;
use ty_module_resolver::{ImportingFile, KnownModule, file_to_module};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};

use crate::types::call::CallError;
use crate::types::context::InferContext;
use crate::types::function::{DataclassTransformerParams, FunctionType};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::DeclaredAndInferredType;
use crate::types::infer::builder::expression_search::contains_string_literal;
use crate::types::known_instance::DeprecatedInstance;
use crate::types::{DataclassParams, KnownClass, StaticClassLiteral, Type, TypeContext};
use crate::{Db, ProgramEnvironment};

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

/// Local work reserved before inspecting source or adding definition-owned results.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) enum ClassDefinitionWork {
    /// Inspects decorator records, class keywords, and the name used for class identity.
    InspectDefinition {
        decorators: usize,
        keywords: usize,
        name_bytes: usize,
    },
    DecoratorExpression,
    MetadataDecorator,
    RuntimeDecorator,
    OriginalClass,
    RecordBinding,
    KeywordExpression,
    BaseExpression,
    RecordDeferred,
}

pub(in crate::types::infer) struct ClassIdentity<'a, 'db> {
    pub(in crate::types::infer) name: &'a Name,
    pub(in crate::types::infer) body_scope: ScopeId<'db>,
    pub(in crate::types::infer) known: Option<KnownClass>,
    pub(in crate::types::infer) deprecated: Option<DeprecatedInstance<'db>>,
    pub(in crate::types::infer) type_check_only: bool,
    pub(in crate::types::infer) dataclass_params: Option<DataclassParams<'db>>,
    pub(in crate::types::infer) dataclass_transformer_params:
        Option<DataclassTransformerParams<'db>>,
    pub(in crate::types::infer) total_ordering: bool,
    pub(in crate::types::infer) has_decorators: bool,
    pub(in crate::types::infer) has_type_params: bool,
    pub(in crate::types::infer) has_explicit_bases: bool,
    pub(in crate::types::infer) has_explicit_metaclass: bool,
}

pub(in crate::types::infer) trait ClassDefinitionEffects<'db>:
    sealed::Sealed
{
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error>;

    async fn checkpoint(&self, work: ClassDefinitionWork) -> Result<(), Self::Error>;

    async fn allocate_vec<T>(&self, capacity: usize) -> Result<Vec<T>, Self::Error>;

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        identity: ClassIdentity<'_, 'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;

    async fn bind_class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
        ty: Type<'db>,
        undecorated_ty: Option<Type<'db>>,
    ) -> Result<(), Self::Error>;

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn infer_class_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error>;

    async fn infer_class_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    async fn class_body_scope(
        &self,
        db: &'db dyn Db,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
    ) -> Result<ScopeId<'db>, Self::Error>;

    async fn known_class(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
        name: &str,
    ) -> Result<Option<KnownClass>, Self::Error>;

    async fn known_module(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
    ) -> Result<Option<KnownModule>, Self::Error>;

    async fn default_dataclass_params(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<DataclassParams<'db>, Self::Error>;

    async fn dataclass_transformer_params(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> Result<Option<DataclassTransformerParams<'db>>, Self::Error>;

    /// Copies stored field-specifier metadata; its size contributes to the admitted work.
    async fn dataclass_params_from_transformer(
        &self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> Result<DataclassParams<'db>, Self::Error>;

    async fn apply_class_decorator(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        decorator_ty: Type<'db>,
        decorated_ty: Type<'db>,
    ) -> Result<Result<Type<'db>, CallError<'db>>, Self::Error>;

    async fn decorator_error_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        error: &CallError<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn is_unknown_decorator_result(
        &self,
        db: &'db dyn Db,
        result_ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn type_retains_original_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        decorated_class: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn class_decorator_preserves_class_binding(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        decorated_class: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn merge_class_preserving_decorator_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        current_binding: Type<'db>,
        decorated_binding: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    /// May walk expression trees, but must not evaluate them or inspect deferred annotations.
    async fn class_bases_contain_string_literal(
        &self,
        class: &ast::StmtClassDef,
    ) -> Result<bool, Self::Error>;
}

pub(in crate::types::infer) struct LegacyInlineEffects;

impl sealed::Sealed for LegacyInlineEffects {}

impl<'db> ClassDefinitionEffects<'db> for LegacyInlineEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error> {
        Ok(context.in_stub())
    }

    fn checkpoint(
        &self,
        _work: ClassDefinitionWork,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    async fn allocate_vec<T>(&self, capacity: usize) -> Result<Vec<T>, Self::Error> {
        Ok(Vec::with_capacity(capacity))
    }

    async fn class_literal(
        &self,
        db: &'db dyn Db,
        identity: ClassIdentity<'_, 'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(StaticClassLiteral::new(
            db,
            identity.name,
            identity.body_scope,
            identity.known,
            identity.deprecated,
            identity.type_check_only,
            identity.dataclass_params,
            identity.dataclass_transformer_params,
            identity.total_ordering,
            identity.has_decorators,
            identity.has_type_params,
            identity.has_explicit_bases,
            identity.has_explicit_metaclass,
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
        builder.undecorated_type = undecorated_ty;
        builder.add_declaration_with_binding(
            class.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
        Ok(())
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder.deferred.insert(definition);
        Ok(())
    }

    fn infer_class_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        decorator: &ast::Decorator,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.infer_decorator(decorator)))
    }

    fn infer_class_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(
            builder.infer_expression(expression, TypeContext::default())
        ))
    }

    fn class_body_scope(
        &self,
        db: &'db dyn Db,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
    ) -> impl Future<Output = Result<ScopeId<'db>, Self::Error>> {
        ready(Ok(scope.to_scope_id(db, builder.program_file())))
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
        name: &str,
    ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>> {
        let importing_file = ImportingFile::File(
            context.file(),
            context.program_environment().resolver_environment(db),
        );
        ready(Ok(KnownClass::try_from_file_and_name(
            db,
            importing_file,
            name,
        )))
    }

    fn known_module(
        &self,
        db: &'db dyn Db,
        context: &InferContext<'db, '_>,
    ) -> impl Future<Output = Result<Option<KnownModule>, Self::Error>> {
        let importing_file = ImportingFile::File(
            context.file(),
            context.program_environment().resolver_environment(db),
        );
        ready(Ok(file_to_module(db, importing_file.resolver_file(db))
            .and_then(|module| module.known(db))))
    }

    fn default_dataclass_params(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<DataclassParams<'db>, Self::Error>> {
        ready(Ok(DataclassParams::default_params(db, env)))
    }

    fn dataclass_transformer_params(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> impl Future<Output = Result<Option<DataclassTransformerParams<'db>>, Self::Error>> {
        ready(Ok(function
            .iter_overloads_and_implementation(db)
            .rev()
            .find_map(|overload| {
                overload.dataclass_transformer_params(db)
            })))
    }

    fn dataclass_params_from_transformer(
        &self,
        db: &'db dyn Db,
        params: DataclassTransformerParams<'db>,
    ) -> impl Future<Output = Result<DataclassParams<'db>, Self::Error>> {
        ready(Ok(DataclassParams::from_transformer_params(db, params)))
    }

    fn apply_class_decorator(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        decorator_ty: Type<'db>,
        decorated_ty: Type<'db>,
    ) -> impl Future<Output = Result<Result<Type<'db>, CallError<'db>>, Self::Error>> {
        ready(Ok(super::apply_class_decorator(
            db,
            env,
            decorator_ty,
            decorated_ty,
        )))
    }

    fn decorator_error_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        error: &CallError<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(error.return_type(db, env)))
    }

    fn is_unknown_decorator_result(
        &self,
        db: &'db dyn Db,
        result_ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(super::is_unknown_decorator_result(db, result_ty)))
    }

    fn type_retains_original_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        decorated_class: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(super::type_retains_original_class(
            db,
            env,
            original_class,
            decorated_class,
        )))
    }

    fn class_decorator_preserves_class_binding(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        decorated_class: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(super::class_decorator_preserves_class_binding(
            db,
            env,
            original_class,
            decorated_class,
        )))
    }

    fn merge_class_preserving_decorator_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original_class: Type<'db>,
        current_binding: Type<'db>,
        decorated_binding: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(super::merge_class_preserving_decorator_result(
            db,
            env,
            original_class,
            current_binding,
            decorated_binding,
        )))
    }

    fn class_bases_contain_string_literal(
        &self,
        class: &ast::StmtClassDef,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(contains_string_literal(class.bases())))
    }
}
