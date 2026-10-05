//! Binding writes retain the declaration and assignment rules of ordinary source inference.

use std::future::{Future, ready};

use ruff_python_ast::{self as ast, AnyNodeRef};
use salsa::execution_probe::FieldRequest;
use ty_python_core::definition::Definition;
use ty_python_core::scope::FileScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{DeclarationsIterator, ImportedFinalCandidatesIterator};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::place::{
    PlaceAndQualifiers, PlaceFromDeclarationsResult, RequiresExplicitReExport,
    module_type_implicit_global_declaration_with, place_from_declarations_with,
};
use crate::reachability::ReachabilityEvaluationCache;
use crate::types::Type;
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::context::InferContext;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::source_binding::{
    BindingWriteOperation, BindingWriteWork, SourceBindingEffects, sealed,
};
use crate::{Db, ProgramEnvironment};

impl sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> SourceBindingEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error> {
        Ok(context.in_stub())
    }

    async fn checkpoint(&self, work: BindingWriteWork) -> Result<(), Self::Error> {
        let units = match work {
            BindingWriteWork::Prepare | BindingWriteWork::InspectPreviousBinding => 1,
            BindingWriteWork::StoreBinding { existing } => {
                existing.checked_add(1).ok_or(Boundary::CostOverflow)?
            }
            BindingWriteWork::ReportConflictingDeclarations { .. } => {
                return Err(unsupported(SourceDefinitionEffect::BindingDiagnostic));
            }
        };
        self.work(units).await
    }

    fn legacy_operation<T>(
        &self,
        operation: BindingWriteOperation,
        _body: impl FnOnce() -> T,
    ) -> impl Future<Output = Result<T, Self::Error>> {
        ready(Err(unsupported(match operation {
            BindingWriteOperation::AssignmentValidation => {
                SourceDefinitionEffect::AssignmentValidation
            }
            BindingWriteOperation::ConflictingDeclarations
            | BindingWriteOperation::FinalReassignment => SourceDefinitionEffect::BindingDiagnostic,
        })))
    }

    async fn declarations(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.work(
            declarations
                .traversal_len()
                .checked_mul(4)
                .and_then(|units| units.checked_add(1))
                .ok_or(Boundary::CostOverflow)?,
        )
        .await?;
        place_from_declarations_with(
            env,
            self,
            declarations,
            RequiresExplicitReExport::No,
            Some(cache),
        )
        .await
    }

    async fn imported_final(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: PlaceFromDeclarationsResult<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        cache: &ReachabilityEvaluationCache<'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.work(
            candidates
                .traversal_len()
                .checked_mul(4)
                .and_then(|units| units.checked_add(1))
                .ok_or(Boundary::CostOverflow)?,
        )
        .await?;
        declared
            .with_imported_final_with(
                env,
                self,
                candidates,
                RequiresExplicitReExport::No,
                Some(cache),
                false,
            )
            .await
    }

    async fn forwarded_assignment_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
        symbol: ScopedSymbolId,
    ) -> Result<Option<(FileScopeId, ScopedSymbolId)>, Self::Error> {
        self.work(1).await?;
        if !scope.is_global() {
            return Err(unsupported(SourceDefinitionEffect::PlaceScope));
        }
        Ok(builder.forwarded_assignment_owner(scope, symbol))
    }

    async fn implicit_module_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        prior: PlaceAndQualifiers<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        let db = builder.db();
        let env = builder.program_environment();
        match prior.into_lookup_result_with(db, env, self).await? {
            Ok(place) => Ok(Ok(place).into()),
            Err(error) => {
                let fallback =
                    module_type_implicit_global_declaration_with(db, env, self, name).await?;
                Ok(error
                    .or_fall_back_to_with(db, env, self, fallback)
                    .await?
                    .into())
            }
        }
    }

    fn fallback_member_declared_type(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _node: AnyNodeRef<'_>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::BindingMember)))
    }

    async fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.work(1).await?;
        if matches!(target, Type::Dynamic(_)) {
            return Ok(true);
        }
        self.legacy_operation(BindingWriteOperation::AssignmentValidation, || {
            builder.validate_assignment_type_legacy(node, binding, declaration, target, value)
        })
        .await
    }

    fn attribute_assignment_transforms_value(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _value: &ast::Expr,
        _attribute: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::BindingMember)))
    }

    fn safe_subscript_assignment(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, '_>,
        _value: &ast::Expr,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::BindingMember)))
    }
}
