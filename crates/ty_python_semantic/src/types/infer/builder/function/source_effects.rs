//! Dependencies of the shared function-definition transaction.
//!
//! An unavailable operation leaves the definition unfinished. In particular, function identity
//! construction needs the preceding name binding even when the function has no decorators.

use std::convert::Infallible;
use std::future::{Future, ready};
use std::ops::ControlFlow;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{FileScopeId, ProgramFile};

use crate::Db;
use crate::types::Type;
use crate::types::function::{
    DataclassTransformerParams, FunctionDecorators, FunctionLiteral, FunctionType, KnownFunction,
    OverloadLiteral,
};
use crate::types::infer::builder::DeclaredAndInferredType;
use crate::types::infer::{
    FunctionDecoratorInference, InferenceFlags, TypeInferenceBuilder, function_known_decorators,
};

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

/// Work performed locally by the shared body, charged before the corresponding operation.
pub(in crate::types::infer) enum FunctionDefinitionWork<'a, 'db> {
    Definition,
    ClassifyDecorator,
    ApplyDecorator,
    DecoratorStorage(usize),
    /// Classification is complete; check whether definition inference may continue.
    DecoratorsClassified {
        definition: Definition<'db>,
        decorators: FunctionDecorators,
        inference_flags: InferenceFlags,
        has_transforming_decorators: bool,
        candidates: &'a [(Type<'db>, &'a ast::Decorator)],
    },
    Parameters(usize),
    OverloadStatement {
        definition: Definition<'db>,
        statement: &'a ast::Stmt,
    },
}

pub(in crate::types::infer) struct OverloadIdentity<'a, 'db> {
    pub(in crate::types::infer) name: &'a Name,
    pub(in crate::types::infer) known: Option<KnownFunction>,
    pub(in crate::types::infer) body_scope: ScopeId<'db>,
    pub(in crate::types::infer) decorators: FunctionDecorators,
    pub(in crate::types::infer) dataclass_transformer: Option<DataclassTransformerParams<'db>>,
    pub(in crate::types::infer) has_return_annotation: bool,
}

pub(in crate::types::infer) struct FunctionDecoratorRequest<'a, 'db> {
    pub(in crate::types::infer) function: &'a ast::StmtFunctionDef,
    pub(in crate::types::infer) definition: Definition<'db>,
    pub(in crate::types::infer) overload_literal: OverloadLiteral<'db>,
    pub(in crate::types::infer) decorator_ty: Type<'db>,
    pub(in crate::types::infer) decorator_node: &'a ast::Decorator,
    pub(in crate::types::infer) inferred_ty: Type<'db>,
    pub(in crate::types::infer) is_decorated_overload_implementation: bool,
}

pub(in crate::types::infer) trait FunctionDefinitionEffects<'db>:
    sealed::Sealed
{
    type Error;

    async fn checkpoint(&self, work: FunctionDefinitionWork<'_, 'db>) -> Result<(), Self::Error>;

    async fn merge_decorator_results(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &FunctionDecoratorInference<'db>,
    ) -> Result<(), Self::Error>;

    async fn decorator_candidates<'ast>(
        &self,
        capacity: usize,
    ) -> Result<Vec<(Type<'db>, &'ast ast::Decorator)>, Self::Error>;

    async fn decorator_type(
        &self,
        inference: Option<&FunctionDecoratorInference<'db>>,
        decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error>;

    async fn classify_decorator(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> Result<FunctionDecorators, Self::Error>;

    async fn check_final_decorators(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        final_decorator: Option<&ast::Decorator>,
        decorators: FunctionDecorators,
    ) -> Result<(), Self::Error>;

    async fn overload_literal(
        &self,
        db: &'db dyn Db,
        identity: OverloadIdentity<'_, 'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error>;

    async fn function_type(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<FunctionType<'db>, Self::Error>;

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn is_overload(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn bind_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;

    async fn report_useless_overload_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        statement: &ast::Stmt,
    ) -> Result<ControlFlow<()>, Self::Error>;

    async fn known_decorators(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<&'db FunctionDecoratorInference<'db>, Self::Error>;

    async fn known_function(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Result<Option<KnownFunction>, Self::Error>;

    async fn function_body_scope(
        &self,
        db: &'db dyn Db,
        program_file: ProgramFile<'db>,
        file_scope: FileScopeId,
    ) -> Result<ScopeId<'db>, Self::Error>;

    /// Uses `FunctionLiteral::new_with` with a provider for completed preceding bindings.
    async fn function_literal(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> Result<FunctionLiteral<'db>, Self::Error>;

    async fn underlying_function(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn check_type_parameter_shadowing(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Self::Error>;

    async fn apply_function_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        request: FunctionDecoratorRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn finish_decorated_overload(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        literal: FunctionLiteral<'db>,
        function: FunctionType<'db>,
        inferred_ty: Type<'db>,
        is_overload_implementation: bool,
        is_overload: bool,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(in crate::types::infer) struct LegacyFunctionDefinitionEffects;

impl sealed::Sealed for LegacyFunctionDefinitionEffects {}

impl<'db> FunctionDefinitionEffects<'db> for LegacyFunctionDefinitionEffects {
    type Error = Infallible;

    fn checkpoint(
        &self,
        _work: FunctionDefinitionWork<'_, 'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    async fn merge_decorator_results(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &FunctionDecoratorInference<'db>,
    ) -> Result<(), Self::Error> {
        builder.extend_function_decorator_inference(inference);
        Ok(())
    }

    async fn decorator_candidates<'ast>(
        &self,
        capacity: usize,
    ) -> Result<Vec<(Type<'db>, &'ast ast::Decorator)>, Self::Error> {
        Ok(Vec::with_capacity(capacity))
    }

    async fn decorator_type(
        &self,
        inference: Option<&FunctionDecoratorInference<'db>>,
        decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(inference
            .and_then(|inference| inference.expression_type(&decorator.expression))
            .unwrap_or_else(Type::unknown))
    }

    async fn classify_decorator(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> Result<FunctionDecorators, Self::Error> {
        Ok(FunctionDecorators::from_decorator_type(builder.db(), ty))
    }

    async fn check_final_decorators(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        final_decorator: Option<&ast::Decorator>,
        decorators: FunctionDecorators,
    ) -> Result<(), Self::Error> {
        builder.check_function_final_decorators(function, final_decorator, decorators);
        Ok(())
    }

    async fn overload_literal(
        &self,
        db: &'db dyn Db,
        identity: OverloadIdentity<'_, 'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error> {
        Ok(OverloadLiteral::new(
            db,
            identity.name,
            identity.known,
            identity.body_scope,
            identity.decorators,
            None,
            identity.dataclass_transformer,
            identity.has_return_annotation,
        ))
    }

    async fn function_type(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Ok(FunctionType::new(db, literal, None))
    }

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(literal.has_separate_implementation(db))
    }

    async fn is_overload(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(overload.is_overload(db))
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder.deferred.insert(definition);
        Ok(())
    }

    async fn bind_function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.add_declaration_with_binding(
            function.into(),
            definition,
            &DeclaredAndInferredType::are_the_same_type(ty),
        );
        Ok(())
    }

    async fn report_useless_overload_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        statement: &ast::Stmt,
    ) -> Result<ControlFlow<()>, Self::Error> {
        Ok(builder.report_useless_overload_body(function, statement))
    }

    fn known_decorators(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db FunctionDecoratorInference<'db>, Self::Error>> {
        ready(Ok(function_known_decorators(builder.db(), definition)))
    }

    fn known_function(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> impl Future<Output = Result<Option<KnownFunction>, Self::Error>> {
        ready(Ok(KnownFunction::try_from_definition_and_name(
            db, definition, name,
        )))
    }

    fn function_body_scope(
        &self,
        db: &'db dyn Db,
        program_file: ProgramFile<'db>,
        file_scope: FileScopeId,
    ) -> impl Future<Output = Result<ScopeId<'db>, Self::Error>> {
        ready(Ok(file_scope.to_scope_id(db, program_file)))
    }

    fn function_literal(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> impl Future<Output = Result<FunctionLiteral<'db>, Self::Error>> {
        ready(Ok(FunctionLiteral::new(db, overload)))
    }

    fn underlying_function(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(ty.underlying_function(db)))
    }

    fn check_type_parameter_shadowing(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready({
            builder.check_function_type_parameter_shadowing(function);
            Ok(())
        })
    }

    fn apply_function_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        request: FunctionDecoratorRequest<'_, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.apply_function_definition_decorator(request)))
    }

    fn finish_decorated_overload(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        literal: FunctionLiteral<'db>,
        function: FunctionType<'db>,
        inferred_ty: Type<'db>,
        is_overload_implementation: bool,
        is_overload: bool,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.finish_function_definition_overload(
            literal,
            function,
            inferred_ty,
            is_overload_implementation,
            is_overload,
        )))
    }
}
