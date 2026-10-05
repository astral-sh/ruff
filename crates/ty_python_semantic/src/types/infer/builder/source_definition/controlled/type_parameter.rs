//! Admitted PEP 695 headers stored in their canonical definition transactions.

use std::future::Future;
use std::pin::Pin;

use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::Type;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::typevar::pep695::{
    TypeParameterDeclarationEffects, TypeParameterDeclarationFacts, TypeParameterDefinitionNode,
    infer_type_parameter_definition_with,
};
use crate::types::infer::type_parameter_header::{
    TypeParameterHeader, TypeParameterHeaderEffects, TypeParameterHeaderFacts,
    TypeParameterHeaderInput, TypeParameterHeaderState, infer_type_parameter_header_with,
};
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity,
    TypeVarInstance, TypeVarKind,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs and returns a boxed child after admitting its fixed storage and transfers.
    /// The factory only constructs the child, which funds variable-sized payloads and their
    /// destruction. See [`Self::boxed_future_with_fixed_transfers`] for capture retention.
    pub(super) async fn type_parameter_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        self.boxed_future_with_fixed_transfers(Ok((0, 0)), make).await
    }

    /// Resolves and infers one parameter through the complete shared declaration algorithm.
    pub(super) async fn infer_type_parameter_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: TypeParameterDefinitionNode<'_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        let node = self
            .local_with_fixed_transfers(12, 0, || node.node(builder.module()))
            .await?;
        self.type_parameter_future(|| {
            infer_type_parameter_definition_with(
                builder,
                definition,
                node,
                TypeParameterDeclarationFacts,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeParameterHeaderEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The header has a fixed number of branch tests, descriptor constructions, and copies.
        // Widths measure their carriers; the work count is independent of those widths.
        let bytes = Self::checked(
            size_of::<TypeParameterHeaderInput<'_>>()
                .checked_mul(4)
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<TypeParameterHeaderState<'db>>().checked_mul(4)?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<TypeParameterHeader<'db>>().checked_mul(4)?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<TypeVarIdentity<'db>>().checked_mul(4)?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<TypeVarInstance<'db>>().checked_mul(4)?)
                })
                .and_then(|bytes| bytes.checked_add(size_of::<Definition<'db>>().checked_mul(4)?)),
        )?;
        self.local_with_fixed_transfers(64, bytes, || ()).await
    }

    async fn intern_identity(
        &self,
        name: &Name,
        definition: Definition<'db>,
        kind: TypeVarKind,
    ) -> RunResult<TypeVarIdentity<'db>> {
        self.type_parameter_future(|| {
            self.access
                .intern_typevar_identity(name, Some(definition), kind)
        })
        .await?
        .await
    }

    async fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.type_parameter_future(|| {
            self.access
                .intern_typevar_instance(identity, bounds, None, default)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> TypeParameterDeclarationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Both optional branches and the declaration conversion are finite. Their carrier
        // transfers are distinct from the interner and storage work in the following effects.
        let bytes = Self::checked(
            size_of::<TypeParameterHeader<'db>>()
                .checked_mul(4)
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<ast::TypeParamRef<'_>>().checked_mul(4)?)
                })
                .and_then(|bytes| bytes.checked_add(size_of::<Type<'db>>().checked_mul(4)?))
                .and_then(|bytes| bytes.checked_add(size_of::<Definition<'db>>().checked_mul(4)?))
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<Option<Definition<'db>>>().checked_mul(4)?)
                })
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<Option<TextRange>>().checked_mul(4)?)
                }),
        )?;
        self.local_with_fixed_transfers(32, bytes, || ()).await
    }

    async fn header(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        node: ast::TypeParamRef<'_>,
    ) -> RunResult<TypeParameterHeader<'db>> {
        let input = self
            .local_with_fixed_transfers(12, 0, || TypeParameterHeaderInput::from(node))
            .await?;
        self.type_parameter_future(|| {
            infer_type_parameter_header_with(definition, input, TypeParameterHeaderFacts, self)
        })
        .await?
        .await
    }

    async fn invalid_constraint_count(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _range: TextRange,
    ) -> RunResult<()> {
        self.type_parameter_future(|| {
            self.unavailable(SourceOperation::TypeParameterConstraintCountDiagnostic)
        })
        .await?
        .await
    }

    async fn record_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.type_parameter_future(|| self.record_source_deferred(builder, definition))
            .await?
            .await
    }

    async fn bind_declaration(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: ast::TypeParamRef<'_>,
        definition: Definition<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let node = self
            .local_with_fixed_transfers(3, 0, || ast::AnyNodeRef::from(node))
            .await?;
        self.type_parameter_future(|| self.bind_source_declaration(builder, node, definition, ty))
            .await?
            .await
    }
}
