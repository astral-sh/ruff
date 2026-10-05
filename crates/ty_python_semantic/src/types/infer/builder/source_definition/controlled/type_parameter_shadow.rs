//! Admitted shadow scans retain canonical contexts and stop at the diagnostic child when matched.

use std::future::Future;
use std::pin::Pin;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::{FileScopeId, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::generics::binding::{BindingFacts, TypeVarBindingEffects};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::shadowing::{
    FunctionShadowEffects, TypeVariableNameEffects, check_function_type_parameter_shadowing_with,
    find_named_typevar_with, function_type_parameter_kind, next_function_type_parameter,
};
use crate::types::infer::TypeInferenceBuilder;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::type_parameter_shadow::{
    self as observations, Stage,
};
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::{BoundTypeVarInstance, GenericContext, TypeVarKind};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits a boxed scan or context child while retaining its borrowed factory until drain.
    /// The quote covers the temporary and boxed future, four output carriers, and retirement;
    /// canonical children fund their own query storage and any variable-sized values.
    pub(super) async fn shadow_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        let bytes = Self::checked(
            size_of::<F>()
                .checked_mul(2)
                .and_then(|bytes| bytes.checked_add(size_of::<F::Output>().checked_mul(4)?)),
        )?;
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::Future);
        self.local_with_fixed_transfers(6, bytes, || {
            let future = Box::pin(make());
            #[cfg(test)]
            observations::observe_after(self.db(), Stage::Future);
            future
        })
        .await
    }

    /// Checks a function's explicit parameters from the scope containing its definition.
    /// A matching name reaches the same reporter effect as ordinary inference, which remains
    /// unavailable here until diagnostic eligibility and publication are admitted.
    pub(super) async fn check_function_type_parameter_shadowing_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        self.check_file_program(builder.program_file()).await?;
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(4, 0, || builder.scope().read_fields(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.file_scope_id())
            .await?;
        let scope = self.field(request).await?;
        self.shadow_future(|| async {
            #[cfg(test)]
            let _scan = observations::scan_enter(self.db());
            let scan =
                check_function_type_parameter_shadowing_with(builder.index, scope, function, self);
            #[cfg(test)]
            let scan = observations::observe_scan_polling(scan);
            scan.await
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVariableNameEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        TypeVarBindingEffects::variables(self, context).await
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::Variable);
        let variable = TypeVarBindingEffects::next_variable(self, variables, cursor).await?;
        #[cfg(test)]
        observations::observe_after(self.db(), Stage::Variable);
        Ok(variable)
    }

    async fn has_name(&self, variable: BoundTypeVarInstance<'db>, name: &Name) -> RunResult<bool> {
        let typevar = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        let identity = TypeVarBindingEffects::typevar_identity(self, typevar).await?;
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || identity.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.name())
            .await?;
        let candidate = self.field(request).await?;
        let work = self
            .local_with_fixed_transfers(8, 3 * size_of::<usize>(), || {
                Self::checked(candidate.len().min(name.len()).checked_add(8))
            })
            .await??;
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::NameComparison);
        self.local_with_fixed_transfers(work, 0, || {
            let matches = candidate == name;
            #[cfg(test)]
            observations::observe_after(self.db(), Stage::NameComparison);
            matches
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionShadowEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next_parameter<'ast>(
        &self,
        function: &'ast ast::StmtFunctionDef,
        cursor: &mut usize,
    ) -> RunResult<Option<&'ast ast::TypeParam>> {
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::Parameter);
        let bytes = size_of::<Option<&ast::TypeParams>>()
            + size_of::<&[ast::TypeParam]>()
            + 2 * size_of::<usize>()
            + size_of::<bool>();
        self.local_with_fixed_transfers(14, bytes, || {
            let parameter = next_function_type_parameter(function, cursor);
            #[cfg(test)]
            observations::observe_after(self.db(), Stage::Parameter);
            parameter
        })
        .await
    }

    async fn parameter_name<'ast>(&self, parameter: &'ast ast::TypeParam) -> RunResult<&'ast Name> {
        self.local_with_fixed_transfers(4, size_of::<&ast::Identifier>(), || &parameter.name().id)
            .await
    }

    async fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> RunResult<AncestorsIter<'index>> {
        TypeVarBindingEffects::ancestors(self, index, scope).await
    }

    async fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> RunResult<Option<(FileScopeId, &'index Scope)>> {
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::Ancestor);
        let ancestor = TypeVarBindingEffects::next_ancestor(self, ancestors).await?;
        #[cfg(test)]
        observations::observe_after(self.db(), Stage::Ancestor);
        Ok(ancestor)
    }

    async fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        scope: &Scope,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let node = self
            .local_with_fixed_transfers(6, size_of::<ReturnCallableTypeVarScope>(), || {
                BindingFacts.node(scope)
            })
            .await?;
        self.shadow_future(|| {
            TypeVarBindingEffects::scope_context(
                self,
                index,
                node,
                ReturnCallableTypeVarScope::Public,
            )
        })
        .await?
        .await
    }

    async fn find_named(
        &self,
        context: GenericContext<'db>,
        name: &Name,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.shadow_future(|| find_named_typevar_with(context, name, self))
            .await?
            .await
    }

    async fn parameter_kind(&self, parameter: &ast::TypeParam) -> RunResult<TypeVarKind> {
        self.local_with_fixed_transfers(3, 0, || function_type_parameter_kind(parameter))
            .await
    }

    async fn report(
        &self,
        _function: &ast::StmtFunctionDef,
        _name: &Name,
        _kind: TypeVarKind,
        _other: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        #[cfg(test)]
        observations::observe_before(self.db(), Stage::Report);
        self.local_with_fixed_transfers(2, 0, || {
            #[cfg(test)]
            observations::observe_after(self.db(), Stage::Report);
        })
        .await?;
        self.unavailable(SourceOperation::FunctionTypeParameterShadowDiagnostic)
            .await
    }
}
