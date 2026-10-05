//! Owned definition transactions evaluated within one source-inference session.
//!
//! A suspended builder keeps its diagnostics private. Only successful finalization produces a
//! definition answer; cancellation and unsupported dependencies discard the entire transaction.

#[cfg(test)]
mod binding;
#[cfg(test)]
mod class;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types::infer) mod controlled;
#[cfg(test)]
mod expression;
#[cfg(test)]
mod function;
#[cfg(test)]
mod implicit_globals;
#[cfg(test)]
mod imports;
#[cfg(test)]
mod place;

#[cfg(test)]
use std::cell::Cell;
use std::convert::Infallible;
use std::future::{Future, ready};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ty_python_core::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, Definition, DefinitionKind,
};
#[cfg(test)]
use ty_python_core::semantic_index;

use super::TypeInferenceBuilder;
use super::typevar::pep695::TypeParameterDefinitionNode;
use crate::Db;
#[cfg(test)]
use crate::ProgramEnvironment;
#[cfg(test)]
use crate::types::callable::scheduled_probe::{Boundary, Router};
#[cfg(test)]
use crate::types::infer::{DefinitionInference, InferenceRegion};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum SourceDefinitionEffect {
    #[cfg(test)]
    FunctionDecorator,
    #[cfg(test)]
    FunctionMetadata,
    #[cfg(test)]
    FunctionShadowing,
    TypeAlias,
    Import,
    ImportFromSubmodule,
    Assignment,
    AnnotatedAssignment,
    AugmentedAssignment,
    DictKeyAssignment,
    For,
    NamedExpression,
    Comprehension,
    Parameter,
    LambdaParameter,
    WithItem,
    MatchPattern,
    ExceptHandler,
    TypeParameter,
    LoopHeaderDefinition,
    NestedBindings,
    #[cfg(any(test, feature = "experimental-analysis"))]
    ClassDecorator,
    #[cfg(any(test, feature = "experimental-analysis"))]
    ClassExpression,
    #[cfg(any(test, feature = "experimental-analysis"))]
    ClassMetadata,
    #[cfg(any(test, feature = "experimental-analysis"))]
    ClassRelation,
    #[cfg(test)]
    BindingDiagnostic,
    #[cfg(test)]
    BindingMember,
    #[cfg(test)]
    AssignmentValidation,
    #[cfg(any(test, feature = "experimental-analysis"))]
    ImplicitModuleClass,
    #[cfg(test)]
    ExpressionOperation(super::source_expression::SourceExpressionOperation),
    #[cfg(test)]
    ExpressionScope,
    #[cfg(test)]
    ImplicitPlace,
    #[cfg(test)]
    ImportPolicy,
    #[cfg(test)]
    ImportDiagnostic,
    #[cfg(test)]
    Submodule,
    #[cfg(test)]
    ModuleGetattr,
    #[cfg(test)]
    ModuleTypeMember,
    #[cfg(test)]
    PlacePromotion,
    #[cfg(test)]
    PlaceScope,
    #[cfg(test)]
    ModuleFallback,
    #[cfg(test)]
    ReExport,
    #[cfg(test)]
    DiscardedBinding,
    #[cfg(test)]
    LoopHeader,
    #[cfg(test)]
    Reachability,
    #[cfg(test)]
    Narrowing,
    #[cfg(test)]
    Union,
    #[cfg(test)]
    FunctionSamePlace,
    #[cfg(test)]
    FunctionContains,
    #[cfg(test)]
    Equivalence,
    #[cfg(test)]
    MissingBinding,
    #[cfg(test)]
    Finalization,
}

#[cfg(test)]
fn unsupported(effect: SourceDefinitionEffect) -> Boundary {
    Boundary::SourceDefinition(effect)
}

pub(super) trait DefinitionEffects<'db> {
    type Error;

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;

    async fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.legacy_operation(SourceDefinitionEffect::Assignment, builder, |builder| {
            builder.infer_assignment_definition(assignment, definition);
        })
        .await
    }

    async fn annotated_assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn parameter<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.legacy_operation(SourceDefinitionEffect::Parameter, builder, |builder| {
            builder.infer_parameter_definition(parameter, definition);
        })
        .await
    }

    async fn type_parameter(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: TypeParameterDefinitionNode<'_>,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.legacy_operation(SourceDefinitionEffect::TypeParameter, builder, |builder| {
            match node.node(builder.module()) {
                ast::TypeParamRef::TypeVar(node) => builder.infer_typevar_definition(node, definition),
                ast::TypeParamRef::ParamSpec(node) => builder.infer_paramspec_definition(node, definition),
                ast::TypeParamRef::TypeVarTuple(node) => builder.infer_typevartuple_definition(node, definition),
            }
        })
        .await
    }

    async fn function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn import_from<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        import: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error>;

    async fn import<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        self.legacy_operation(SourceDefinitionEffect::Import, builder, |builder| {
            builder.infer_import_definition(alias, definition);
        })
        .await
    }

    async fn legacy_operation<'ast>(
        &self,
        effect: SourceDefinitionEffect,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        body: impl FnOnce(&mut TypeInferenceBuilder<'db, 'ast>),
    ) -> Result<(), Self::Error>;
}

pub(super) struct LegacyDefinitionEffects;

impl<'db> DefinitionEffects<'db> for LegacyDefinitionEffects {
    type Error = Infallible;

    fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db DefinitionKind<'db>, Self::Error>> {
        ready(Ok(definition.kind(db)))
    }

    fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.infer_assignment_definition(assignment, definition);
        ready(Ok(()))
    }

    fn annotated_assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.infer_annotated_assignment_definition(assignment, definition);
        ready(Ok(()))
    }

    fn function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.infer_function_definition(function, definition);
        ready(Ok(()))
    }

    fn class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.infer_class_definition(class, definition);
        ready(Ok(()))
    }

    fn import_from<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        import: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.infer_import_from_definition(import, alias, definition);
        ready(Ok(()))
    }

    fn legacy_operation<'ast>(
        &self,
        _effect: SourceDefinitionEffect,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        body: impl FnOnce(&mut TypeInferenceBuilder<'db, 'ast>),
    ) -> impl Future<Output = Result<(), Self::Error>> {
        body(builder);
        ready(Ok(()))
    }
}

#[cfg(test)]
struct QueuedDefinitionEffects<'eval, 'db, 'c> {
    db: &'db dyn Db,
    router: &'eval Router<'db, 'c>,
    owner: Definition<'db>,
    sequence: Cell<usize>,
}

#[cfg(test)]
impl QueuedDefinitionEffects<'_, '_, '_> {
    async fn work(&self, units: usize) -> Result<(), Boundary> {
        let sequence = self.sequence.get();
        self.sequence
            .set(sequence.checked_add(1).ok_or(Boundary::CostOverflow)?);
        self.router
            .source_checkpoint(self.owner, sequence, units)
            .await
    }
}

#[cfg(test)]
impl<'db> DefinitionEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db DefinitionKind<'db>, Self::Error>> {
        ready(Ok(definition.kind(db)))
    }

    fn annotated_assignment<'ast>(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _assignment: &'db AnnotatedAssignmentDefinitionKind,
        _definition: Definition<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(
            SourceDefinitionEffect::AnnotatedAssignment,
        )))
    }

    async fn function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .infer_function_definition_with(self, function, definition)
            .await
    }

    async fn class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .infer_class_definition_with(self, class, definition)
            .await
    }

    async fn import_from<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        import: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> Result<(), Self::Error> {
        builder
            .infer_import_from_definition_with(self, import, alias, definition)
            .await
    }

    fn legacy_operation<'ast>(
        &self,
        effect: SourceDefinitionEffect,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _body: impl FnOnce(&mut TypeInferenceBuilder<'db, 'ast>),
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(unsupported(effect)))
    }
}

#[cfg(test)]
thread_local! {
    static LIVE_BUILDERS: Cell<usize> = const { Cell::new(0) };
    static CREATED_BUILDERS: Cell<usize> = const { Cell::new(0) };
    static DROPPED_BUILDERS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn scheduled_definition_live_builders() -> usize {
    LIVE_BUILDERS.get()
}

#[cfg(test)]
pub(crate) fn scheduled_definition_builder_counts() -> (usize, usize) {
    (CREATED_BUILDERS.get(), DROPPED_BUILDERS.get())
}

#[cfg(test)]
struct LiveBuilder;

#[cfg(test)]
impl Drop for LiveBuilder {
    fn drop(&mut self) {
        LIVE_BUILDERS.set(LIVE_BUILDERS.get() - 1);
        DROPPED_BUILDERS.set(DROPPED_BUILDERS.get() + 1);
    }
}

#[cfg(test)]
pub(crate) async fn evaluate_scheduled_definition<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    definition: Definition<'db>,
) -> Result<Arc<DefinitionInference<'db>>, Boundary> {
    if definition.program(db) != env.program(db) {
        return Err(Boundary::ProgramDomain);
    }
    let file = definition.program_file(db);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    let source_env = ProgramEnvironment::from_file(file);
    let effects = QueuedDefinitionEffects {
        db,
        router,
        owner: definition,
        sequence: Cell::new(0),
    };
    let mut builder = TypeInferenceBuilder::new(
        db,
        &source_env,
        InferenceRegion::Definition(definition),
        file.file(db),
        file,
        index,
        &module,
    );
    // The owning future publishes only a finalized answer, so a dropped transaction may discard
    // its diagnostics. Ordinary synchronous builders retain their diagnostic-loss check.
    builder.context.defuse();
    #[cfg(test)]
    let _live_builder = {
        LIVE_BUILDERS.set(LIVE_BUILDERS.get() + 1);
        CREATED_BUILDERS.set(CREATED_BUILDERS.get() + 1);
        LiveBuilder
    };
    effects.work(1).await?;
    builder
        .infer_region_definition_with(&effects, definition)
        .await?;
    // The admitted source paths store fixed-size bindings, declarations, expressions and deferred IDs.
    // Other finalizer payloads need their own size-aware reservation before they are admitted.
    if builder.context.has_diagnostics()
        || !builder.comparison_truthiness.is_empty()
        || !builder.qualifiers.is_empty()
        || !builder.type_expression_flags.is_empty()
        || !builder.collection_use_constraints.is_empty()
        || !builder.string_annotations.is_empty()
        || !builder.expected_types.is_empty()
        || !builder.implicit_aliases.is_empty()
        || !builder.called_functions.is_empty()
        || !builder.deferred_decorator_calls.is_empty()
        || builder.cycle_recovery.is_some()
    {
        return Err(unsupported(SourceDefinitionEffect::Finalization));
    }
    // FrozenMap sorts expression keys and consumes the hash table's retained allocation.
    // Both costs belong to finalization, after expression inference has already completed.
    let expressions = builder.expressions.len();
    let sort_levels = (usize::BITS - expressions.leading_zeros()) as usize;
    let expression_work = expressions
        .checked_mul(sort_levels + 1)
        .and_then(|work| work.checked_mul(4))
        .and_then(|work| work.checked_add(builder.expressions.capacity()))
        .ok_or(Boundary::CostOverflow)?;
    let work = builder
        .bindings
        .len()
        .checked_add(builder.declarations.len())
        .and_then(|work| work.checked_add(expression_work))
        .and_then(|work| work.checked_add(builder.deferred.0.len()))
        .and_then(|work| work.checked_add(8))
        .ok_or(Boundary::CostOverflow)?;
    effects.work(work).await?;
    Ok(Arc::new(builder.finish_inferred_definition(definition)))
}
