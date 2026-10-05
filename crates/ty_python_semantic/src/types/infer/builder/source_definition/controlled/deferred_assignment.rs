use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::storage::sequence_merge;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::DeferredInferenceOperation;
use crate::types::function::KnownFunction;
use crate::types::infer::InferenceRegion;
use crate::types::infer::builder::deferred::assignment::{
    DeferredAssignmentChild, DeferredAssignmentEffects, validate_typevar_default_with,
};
use crate::types::infer::builder::type_expression::TypeExpressionMode;
use crate::types::infer::builder::{BoundOrConstraintsNodes, TypeInferenceBuilder, local};
use crate::types::typevar::{TypeVarBoundOrConstraints, TypeVarConstraints};
use crate::types::visitor::runtime::{RuntimeTypeSearch, RuntimeTypeWalk};
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{KnownClass, KnownInstanceType, Type, TypeContext, TypingModule};

struct HasTypeVarOrInstance;

impl<'db> RuntimeTypeSearch<'db> for HasTypeVarOrInstance {
    fn predicate(&self, ty: Type<'db>) -> bool {
        matches!(
            ty,
            Type::KnownInstance(KnownInstanceType::TypeVar(_)) | Type::TypeVar(_)
        )
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> DeferredAssignmentEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn cached_expression(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Option<Type<'db>>> {
        let work = Self::checked(builder.expressions.len().checked_add(4))?;
        self.local(work, 0, || builder.try_expression_type(expression))
            .await
    }

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        local::source::expression(builder, expression, TypeContext::default(), self).await
    }

    async fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.work(2).await?;
        self.known_call_class(builder.db(), ty).await
    }

    async fn deferred_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(1, 0, || match builder.region {
            InferenceRegion::Deferred(definition) => Some(definition),
            _ => None,
        })
        .await
    }

    async fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<TypingModule>> {
        self.typed_dict_call_module(builder, ty).await
    }

    async fn is_new_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.call_function_is_known(builder, ty, KnownFunction::NewClass)
            .await
    }

    async fn child(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _value: &'ast ast::Expr,
        _call: &'ast ast::ExprCall,
        child: DeferredAssignmentChild<'db>,
    ) -> RunResult<()> {
        let operation = match child {
            DeferredAssignmentChild::NamedTuple => DeferredInferenceOperation::AssignmentNamedTuple,
            DeferredAssignmentChild::NewType => DeferredInferenceOperation::AssignmentNewType,
            DeferredAssignmentChild::TypeAliasType(_) => {
                DeferredInferenceOperation::AssignmentTypeAliasType
            }
            DeferredAssignmentChild::BuiltinType(_) => {
                DeferredInferenceOperation::AssignmentBuiltinType
            }
            DeferredAssignmentChild::TypedDict => DeferredInferenceOperation::AssignmentTypedDict,
            DeferredAssignmentChild::NewClass(_) => DeferredInferenceOperation::AssignmentNewClass,
        };
        self.unavailable(SourceOperation::Deferred(operation)).await
    }

    async fn new_constraints(&self) -> RunResult<Vec<Type<'db>>> {
        self.local(1, 0, Vec::new).await
    }

    async fn next_constraint(
        &self,
        call: &'ast ast::ExprCall,
        cursor: &mut usize,
    ) -> RunResult<Option<&'ast ast::Expr>> {
        self.local(2, 0, || {
            let constraint = call.arguments.args.get(*cursor)?;
            *cursor += 1;
            Some(constraint)
        })
        .await
    }

    async fn push_constraint(
        &self,
        constraints: &mut Vec<Type<'db>>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let quote = sequence_merge::<Type<'db>>(constraints.len(), constraints.capacity(), 1)
            .ok_or(RunError::Contract(
                "deferred constraint buffer quotation overflow",
            ))?;
        let disposal = if constraints.len() == constraints.capacity() {
            Self::checked(constraints.capacity().checked_mul(2))?.max(4)
        } else {
            0
        };
        let work = Self::checked(quote.work.checked_add(disposal))?;
        self.local(work, quote.bytes, || constraints.push(ty)).await
    }

    async fn intern_constraints(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _constraints: Vec<Type<'db>>,
    ) -> RunResult<TypeVarConstraints<'db>> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::AssignmentConstraints,
        ))
        .await
    }

    async fn type_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let ty =
            local::source::type_expression(builder, expression, TypeExpressionMode::Scoped, self)
                .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::deferred_assignments::observe_type_expression(
            self.db(),
            ty,
        );
        Ok(ty)
    }

    async fn has_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(builder.program_environment())
            .await?;
        let mut walk = RuntimeTypeWalk {
            db: self.db(),
            endpoint: self.access.endpoint(),
            query: HasTypeVarOrInstance,
            unavailable: self,
        };
        search_type_with(
            ty,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut walk,
        )
        .await
    }

    async fn generic_constraint(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::AssignmentConstraintDiagnostic,
        ))
        .await
    }

    async fn generic_bound(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _bound: &ast::Keyword,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::AssignmentBoundDiagnostic,
        ))
        .await
    }

    async fn find_keyword(
        &self,
        call: &'ast ast::ExprCall,
        name: &str,
    ) -> RunResult<Option<&'ast ast::Keyword>> {
        let work = Self::checked(
            name.len()
                .checked_add(4)
                .and_then(|per_keyword| call.arguments.keywords.len().checked_mul(per_keyword))
                .and_then(|work| work.checked_add(1)),
        )?;
        self.local(work, 0, || call.arguments.find_keyword(name))
            .await
    }

    async fn paramspec_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        self.infer_paramspec_default_source(builder, expression, None).await
    }

    async fn typevartuple_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        self.infer_typevartuple_default_source(builder, expression, None).await
    }

    async fn validate_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: Option<&str>,
        bounds: Option<TypeVarBoundOrConstraints<'db>>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> RunResult<()> {
        validate_typevar_default_with(
            builder,
            name,
            bounds,
            default_ty,
            default_node,
            bound_nodes,
            self,
        )
        .await
    }

    async fn bounded_default(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _name: Option<&str>,
        _bounds: TypeVarBoundOrConstraints<'db>,
        _default_ty: Type<'db>,
        _default_node: &ast::Expr,
        _bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(
            DeferredInferenceOperation::AssignmentBoundedDefault,
        ))
        .await
    }
}
