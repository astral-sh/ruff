use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ScopedDefinitionId;
use ty_python_core::definition::Definition;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::FileScopeId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::source_binding::SourceBindingEffects;
use crate::types::infer::builder::typevar::legacy::{
    self, Candidate, Candidates, HeaderDiagnostic, LegacyTypeVarEffects,
};
use crate::types::literal::StringLiteralValueDeref;
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::{Truthiness, Type, TypeVarKind, TypeVarVariance};

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> LegacyTypeVarEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        SourceBindingEffects::in_stub(self, &builder.context).await
    }

    async fn python_version(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<PythonVersion> {
        let program = self
            .environment_program(builder.program_environment())
            .await?;
        let fields = self.access.endpoint().field_request_context();
        let environment = self
            .field(program.field_requests(fields).resolver_environment())
            .await?;
        self.field(environment.read_fields(fields).python_version())
            .await
    }

    async fn next_argument<'expr>(
        &self,
        call: &'expr ast::ExprCall,
        cursor: &mut usize,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local(2, 0, || legacy::next_argument(call, cursor))
            .await
    }

    async fn next_keyword<'expr>(
        &self,
        call: &'expr ast::ExprCall,
        cursor: &mut usize,
    ) -> RunResult<Option<&'expr ast::Keyword>> {
        let length = call
            .arguments
            .keywords
            .get(*cursor)
            .and_then(|keyword| keyword.arg.as_ref())
            .map_or(0, |name| name.id().len());
        self.local(Self::checked(length.checked_add(4))?, 0, || {
            legacy::next_keyword(call, cursor)
        })
        .await
    }

    async fn truthiness(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness> {
        self.type_truthiness(builder.program_environment(), ty)
            .await
    }

    async fn string_value(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<&'db str>> {
        let Some(literal) = ty.as_string_literal() else {
            return Ok(None);
        };
        let value = self
            .field_with_profile(
                literal
                    .field_requests(self.access.endpoint().field_request_context())
                    .value(),
                &StringLiteralValueDeref,
            )
            .await?;
        Ok(Some(value))
    }

    async fn same_name(&self, actual: &str, expected: &Name) -> RunResult<bool> {
        self.local(
            Self::checked(
                actual
                    .len()
                    .checked_add(expected.len())
                    .and_then(|n| n.checked_add(1)),
            )?,
            0,
            || actual == expected,
        )
        .await
    }

    async fn error(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _diagnostic: HeaderDiagnostic<'_>,
        _range: TextRange,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::LegacyTypeVarDiagnostic)
            .await
    }

    async fn mismatched_name(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _range: TextRange,
        _expected: &Name,
        _actual: &str,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::LegacyTypeVarDiagnostic)
            .await
    }

    async fn redefinition(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _name: &Name,
        _previous: Definition<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::LegacyTypeVarDiagnostic)
            .await
    }

    async fn mark_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.record_source_deferred(builder, definition).await
    }

    async fn intern_identity(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &Name,
        definition: Definition<'db>,
    ) -> RunResult<TypeVarIdentity<'db>> {
        self.access
            .intern_typevar_identity(name, Some(definition), TypeVarKind::LegacyTypeVar)
            .await
    }

    async fn intern_typevar(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.access
            .intern_typevar_instance(identity, bounds, variance, default)
            .await
    }

    async fn scope_place(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<(FileScopeId, ScopedPlaceId)> {
        let fields = self.access.endpoint().field_request_context();
        let scope = self
            .field(definition.read_fields(fields).scope_id())
            .await?;
        let scope = self
            .field(scope.read_fields(fields).file_scope_id())
            .await?;
        let place = self
            .field(definition.read_fields(fields).place_info())
            .await?;
        Ok((scope, place.place()))
    }

    async fn candidates(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
        declarations: bool,
    ) -> RunResult<Candidates<'db>> {
        self.local(4, 0, || {
            Candidates::new(builder, scope, place, declarations)
        })
        .await
    }

    async fn next_candidate(
        &self,
        candidates: &mut Candidates<'db>,
    ) -> RunResult<Option<Candidate<'db>>> {
        self.local(Self::checked(candidates.step_work())?, 0, || {
            candidates.next()
        })
        .await
    }

    async fn reachable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        reachability: ScopedReachabilityConstraintId,
    ) -> RunResult<bool> {
        let use_def = self
            .local(1, 0, || builder.index.use_def_map(scope))
            .await?;
        Ok(self
            .evaluate_reachability(
                use_def.reachability_constraints(),
                use_def.predicates(),
                reachability,
            )
            .await?
            .may_be_true())
    }

    async fn user_visible(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let kind = self
            .field(
                definition
                    .read_fields(self.access.endpoint().field_request_context())
                    .kind(),
            )
            .await?;
        self.local(1, 0, || kind.is_user_visible()).await
    }

    async fn forwarded_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
    ) -> RunResult<Option<(FileScopeId, ScopedPlaceId)>> {
        let local = self
            .local(2, 0, || {
                scope.is_global()
                    || builder
                        .index
                        .place_table(scope)
                        .symbol(place.expect_symbol())
                        .is_local()
            })
            .await?;
        if local {
            return Ok(None);
        }
        self.unavailable(SourceOperation::LegacyTypeVarForwardedOwner)
            .await
    }

    async fn forwarded_before(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _scope: FileScopeId,
        _owner_scope: FileScopeId,
        _owner_place: ScopedPlaceId,
    ) -> RunResult<Option<ScopedDefinitionId>> {
        self.unavailable(SourceOperation::LegacyTypeVarForwardedOwner)
            .await
    }

    async fn previous_in(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
        before: ScopedDefinitionId,
    ) -> RunResult<Option<Definition<'db>>> {
        legacy::previous_in_with(builder, scope, place, before, legacy::HeaderFacts, self).await
    }

    async fn previous(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        legacy::previous_with(builder, definition, legacy::HeaderFacts, self).await
    }
}
