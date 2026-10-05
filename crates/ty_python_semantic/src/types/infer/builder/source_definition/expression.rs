//! Expression dependencies admitted by an owned definition transaction.
//!
//! Global Name loads follow the shared lexical resolver and place reducers. Each definition
//! contributing a binding is read through the owning session's completed-definition table.

use std::future::{Future, ready};

use ruff_python_ast as ast;
use salsa::execution_probe::FieldRequest;
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::place::{PlaceExpr, ScopedPlaceId};
use ty_python_core::scope::ScopeId;

use super::{QueuedDefinitionEffects, SourceDefinitionEffect, unsupported};
use crate::place::source_effects::SourcePlaceEffects;
use crate::place::{
    ConsideredDefinitions, LookupError, LookupResult, Place, PlaceAndQualifiers,
    RequiresExplicitReExport, place_from_bindings_with,
};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadMode, PlaceLoadResolution, PlaceLoadResolutionStep,
    resolve_place_load,
};
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::class::ClassLiteral;
use crate::types::function::FunctionType;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::source_expression::{
    SourceExpressionEffects, SourceExpressionOperation, SourceExpressionWork, sealed,
};
use crate::types::known_instance::DeprecatedInstance;

impl sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl SourceExpressionWork {
    pub(in crate::types::infer) fn units(self) -> Option<usize> {
        match self {
            SourceExpressionWork::Expression
            | SourceExpressionWork::BoundMethod
            | SourceExpressionWork::PlaceSource => Some(1),
            SourceExpressionWork::StoreExpression { entries, capacity } => {
                // Expression keys have fixed size, and the queued Name path only inserts.
                // Rebuilding the table needs an allowance only when the table is full.
                if entries == capacity {
                    entries.checked_add(1)
                } else {
                    Some(1)
                }
            }
            SourceExpressionWork::NameBytes(length)
            | SourceExpressionWork::NarrowingConstraints(length) => length.checked_add(1),
        }
    }
}

impl<'db> SourceExpressionEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn store_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
        ty: crate::types::Type<'db>,
    ) -> Result<(), Self::Error> {
        self.checkpoint(SourceExpressionWork::StoreExpression {
            entries: builder.expressions.len(),
            capacity: builder.expressions.capacity(),
        })
        .await?;
        builder.store_expression_type(expression, ty);
        Ok(())
    }

    async fn checkpoint(&self, work: SourceExpressionWork) -> Result<(), Self::Error> {
        self.work(work.units().ok_or(Boundary::CostOverflow)?).await
    }

    fn legacy_operation<T>(
        &self,
        operation: SourceExpressionOperation,
        _body: impl FnOnce() -> T,
    ) -> impl Future<Output = Result<T, Self::Error>> {
        ready(Err(unsupported(
            SourceDefinitionEffect::ExpressionOperation(operation),
        )))
    }

    async fn prepare_place_resolution<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceExpr,
        mode: PlaceLoadMode<'expr>,
    ) -> Result<PlaceLoadResolution<'db, 'expr>, Self::Error> {
        let db = builder.db();
        let scope = builder.scope();
        if scope.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        let PlaceExpr::Symbol(symbol) = &place else {
            return Err(unsupported(SourceDefinitionEffect::ExpressionScope));
        };
        if !scope.file_scope_id(db).is_global()
            || matches!(mode, PlaceLoadMode::AtExpression(expression) if !matches!(expression, ast::ExprRef::Name(_)))
        {
            return Err(unsupported(SourceDefinitionEffect::ExpressionScope));
        }

        // A global symbol visits a fixed number of resolver nodes and has no enclosing scopes
        // to traverse. Reserve its name lookups, copies and constraint entries before creating
        // or advancing the resolver. Nested scopes require a separate traversal allowance.
        let units = symbol
            .name()
            .len()
            .checked_add(1)
            .and_then(|units| units.checked_mul(32))
            .ok_or(Boundary::CostOverflow)?;
        self.work(units).await?;
        Ok(resolve_place_load(db, builder.index, scope, place, mode))
    }

    async fn next_place_resolution(
        &self,
        resolution: &mut PlaceLoadResolution<'db, '_>,
    ) -> Result<Option<PlaceLoadResolutionStep<'db>>, Self::Error> {
        self.work(1).await?;
        Ok(resolution.next())
    }

    async fn bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Result<Place<'db>, Self::Error> {
        let units = bindings
            .traversal_len()
            .checked_mul(4)
            .and_then(|units| units.checked_add(4))
            .ok_or(Boundary::CostOverflow)?;
        self.work(units).await?;
        Ok(place_from_bindings_with(
            builder.program_environment(),
            self,
            bindings,
            RequiresExplicitReExport::No,
            Some(builder.reachability_cache()),
        )
        .await?
        .place)
    }

    async fn owning_scope_symbol(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: ScopeId<'db>,
        id: ScopedPlaceId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        SourcePlaceEffects::place_by_id(
            self,
            builder.db(),
            scope,
            id,
            RequiresExplicitReExport::No,
            ConsideredDefinitions::AllReachable,
        )
        .await
    }

    fn implicit_place(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _implicit: ImplicitPlaceLoad<'db>,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Err(unsupported(SourceDefinitionEffect::ImplicitPlace)))
    }

    async fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        place: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error> {
        self.work(1).await?;
        place
            .into_lookup_result_with(builder.db(), builder.program_environment(), self)
            .await
    }

    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        primary: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.work(1).await?;
        Ok(primary
            .or_fall_back_to_with(builder.db(), builder.program_environment(), self, fallback)
            .await?
            .into())
    }

    async fn class_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<DeprecatedInstance<'db>>, Self::Error> {
        let db = builder.db();
        if class.program_file(db).program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(class.deprecated(db))
    }

    async fn function_deprecation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        function: FunctionType<'db>,
    ) -> Result<Option<DeprecatedInstance<'db>>, Self::Error> {
        let db = builder.db();
        if function.program_file(db).program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        function.implementation_deprecated_with(db, self).await
    }
}
