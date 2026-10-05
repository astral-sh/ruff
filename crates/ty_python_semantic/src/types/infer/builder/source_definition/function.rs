//! Function identities supplied by a completed source transaction.
//!
//! The preceding name use is reduced before constructing the identity, so an unfinished earlier
//! definition cannot be mistaken for the absence of an overload.

use std::future::{Future, ready};
use std::ops::ControlFlow;

use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use salsa::execution_probe::FieldRequest;
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{FileScopeId, ProgramFile, semantic_index};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::place::{Place, RequiresExplicitReExport, place_from_bindings_with};
use crate::types::Type;
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::function::{
    FunctionDecorators, FunctionIdentityEffects, FunctionLiteral, FunctionMetadataEffects,
    FunctionType, KnownFunction, OverloadLiteral, identity_sealed,
};
use crate::types::infer::builder::function::source_effects::{
    FunctionDecoratorRequest, FunctionDefinitionEffects, FunctionDefinitionWork,
    LegacyFunctionDefinitionEffects, OverloadIdentity, sealed,
};
use crate::types::infer::{FunctionDecoratorInference, TypeInferenceBuilder};
use crate::types::signatures::effects::legacy_inline;
use crate::{Db, ProgramEnvironment};

impl sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> FunctionDefinitionEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn checkpoint(&self, work: FunctionDefinitionWork<'_, 'db>) -> Result<(), Self::Error> {
        let units = match work {
            FunctionDefinitionWork::Definition => 1,
            FunctionDefinitionWork::DecoratorStorage(count) => {
                count.checked_add(1).ok_or(Boundary::CostOverflow)?
            }
            FunctionDefinitionWork::Parameters(count) => count
                .checked_mul(2)
                .and_then(|count| count.checked_add(1))
                .ok_or(Boundary::CostOverflow)?,
            FunctionDefinitionWork::ClassifyDecorator | FunctionDefinitionWork::ApplyDecorator => {
                return Err(unsupported(SourceDefinitionEffect::FunctionDecorator));
            }
            FunctionDefinitionWork::DecoratorsClassified {
                decorators,
                has_transforming_decorators,
                candidates,
                ..
            } => {
                if !decorators.is_empty() || has_transforming_decorators || !candidates.is_empty() {
                    return Err(unsupported(SourceDefinitionEffect::FunctionDecorator));
                }
                1
            }
            FunctionDefinitionWork::OverloadStatement { .. } => {
                return Err(unsupported(SourceDefinitionEffect::FunctionMetadata));
            }
        };
        self.work(units).await
    }

    async fn merge_decorator_results(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _inference: &FunctionDecoratorInference<'db>,
    ) -> Result<(), Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionDecorator))
    }

    async fn decorator_candidates<'ast>(
        &self,
        capacity: usize,
    ) -> Result<Vec<(Type<'db>, &'ast ast::Decorator)>, Self::Error> {
        if capacity != 0 {
            return Err(unsupported(SourceDefinitionEffect::FunctionDecorator));
        }
        self.work(1).await?;
        Ok(Vec::new())
    }

    async fn decorator_type(
        &self,
        _inference: Option<&FunctionDecoratorInference<'db>>,
        _decorator: &ast::Decorator,
    ) -> Result<Type<'db>, Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionDecorator))
    }

    async fn classify_decorator(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _ty: Type<'db>,
    ) -> Result<FunctionDecorators, Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionDecorator))
    }

    async fn check_final_decorators(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _function: &ast::StmtFunctionDef,
        final_decorator: Option<&ast::Decorator>,
        decorators: FunctionDecorators,
    ) -> Result<(), Self::Error> {
        if final_decorator.is_some()
            || decorators.contains(FunctionDecorators::ABSTRACT_METHOD | FunctionDecorators::FINAL)
        {
            return Err(unsupported(SourceDefinitionEffect::FunctionMetadata));
        }
        self.work(1).await
    }

    async fn overload_literal(
        &self,
        db: &'db dyn Db,
        identity: OverloadIdentity<'_, 'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error> {
        self.work(1).await?;
        Ok(legacy_inline(
            LegacyFunctionDefinitionEffects.overload_literal(db, identity),
        ))
    }
    async fn function_type(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        self.work(1).await?;
        Ok(FunctionType::new(db, literal, None))
    }

    async fn has_separate_implementation(
        &self,
        _db: &'db dyn Db,
        _literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionMetadata))
    }

    async fn is_overload(
        &self,
        _db: &'db dyn Db,
        _overload: OverloadLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionMetadata))
    }
    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.work(1).await?;
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
        self.work(2).await?;
        legacy_inline(
            LegacyFunctionDefinitionEffects.bind_function(builder, function, definition, ty),
        );
        Ok(())
    }

    async fn report_useless_overload_body(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _function: &ast::StmtFunctionDef,
        _statement: &ast::Stmt,
    ) -> Result<ControlFlow<()>, Self::Error> {
        Err(unsupported(SourceDefinitionEffect::FunctionMetadata))
    }

    fn known_decorators(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db FunctionDecoratorInference<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionDecorator)))
    }

    async fn known_function(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Result<Option<KnownFunction>, Self::Error> {
        if definition.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(name.len().checked_add(1).ok_or(Boundary::CostOverflow)?)
            .await?;
        Ok(KnownFunction::try_from_definition_and_name(
            db, definition, name,
        ))
    }

    async fn function_body_scope(
        &self,
        db: &'db dyn Db,
        program_file: ProgramFile<'db>,
        file_scope: FileScopeId,
    ) -> Result<ScopeId<'db>, Self::Error> {
        if program_file.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(file_scope.to_scope_id(db, program_file))
    }

    async fn function_literal(
        &self,
        db: &'db dyn Db,
        overload: OverloadLiteral<'db>,
    ) -> Result<FunctionLiteral<'db>, Self::Error> {
        FunctionLiteral::new_with(db, overload, self).await
    }

    fn underlying_function(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionDecorator)))
    }

    fn check_type_parameter_shadowing(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _function: &ast::StmtFunctionDef,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionShadowing)))
    }

    fn apply_function_decorator(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _request: FunctionDecoratorRequest<'_, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionDecorator)))
    }

    fn finish_decorated_overload(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _literal: FunctionLiteral<'db>,
        _function: FunctionType<'db>,
        _inferred_ty: Type<'db>,
        _is_overload_implementation: bool,
        _is_overload: bool,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionMetadata)))
    }
}

impl identity_sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> FunctionIdentityEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn names_equal(
        &self,
        left: &ast::name::Name,
        right: &ast::name::Name,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    async fn definition(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
    ) -> Result<Definition<'db>, Self::Error> {
        if function.program_file(db).program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(function.definition(db))
    }

    async fn preceding_bindings(
        &self,
        db: &'db dyn Db,
        function: OverloadLiteral<'db>,
        definition: Definition<'db>,
    ) -> Result<Place<'db>, Self::Error> {
        if definition.program(db) != self.owner.program(db)
            || function.program_file(db).program(db) != self.owner.program(db)
        {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        let scope = definition.scope(db);
        let module = parsed_module(db, function.python_file(db)).load(db);
        let use_def =
            semantic_index(db, scope.program_file(db)).use_def_map(scope.file_scope_id(db));
        let use_id = function
            .body_scope(db)
            .node(db)
            .expect_function()
            .node(&module)
            .name
            .scoped_use_id(db, function.program_file(db));
        let bindings = use_def.bindings_at_use(use_id);
        let units = bindings
            .traversal_len()
            .checked_mul(4)
            .and_then(|count| count.checked_add(4))
            .ok_or(Boundary::CostOverflow)?;
        self.work(units).await?;
        let env = ProgramEnvironment::from_scope(scope);
        Ok(
            place_from_bindings_with(&env, self, bindings, RequiresExplicitReExport::No, None)
                .await?
                .place,
        )
    }

    async fn callable_definition(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<Option<FunctionLiteral<'db>>, Self::Error> {
        if definition.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        let inference = self
            .router
            .definition_demand(self.owner, definition)
            .await?;
        self.work(1).await?;
        Ok(inference
            .function_type(definition)
            .map(|function| function.literal(db)))
    }
}

impl<'db> FunctionMetadataEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    fn overloads_and_implementation(
        &self,
        _db: &'db dyn Db,
        _last_definition: OverloadLiteral<'db>,
    ) -> impl Future<
        Output = Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>,
    > {
        ready(Err(unsupported(SourceDefinitionEffect::FunctionMetadata)))
    }
}
