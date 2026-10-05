//! Assignment statements retain shared value bindings and infer targets in source order.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::expression::Expression;
use ty_python_core::unpack::Unpack;

use super::TypeInferenceBuilder;
use crate::types::TypeContext;
use crate::types::infer::{InferenceRegion, infer_expression_types, infer_unpack_types};

pub(in crate::types::infer::builder) struct AssignmentStatementFacts;
pub(in crate::types::infer::builder) struct OrdinaryAssignmentStatementEffects;

shared_semantic_family! {
    #[synchronous(SynchronousAssignmentStatementEffects)]
    pub(in crate::types::infer::builder) trait AssignmentStatementEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn infer_name(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: &ast::ExprName) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn shared_value(&self, builder: &TypeInferenceBuilder<'db, 'ast>, value: &ast::Expr) -> Result<Expression<'db>, Self::Error>;
        #[operation(child)]
        async fn retain_value_bindings(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: Expression<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_target<'target>(&self, targets: &'target [ast::Expr], cursor: &mut usize) -> Result<Option<&'target ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr) -> Result<Option<Unpack<'db>>, Self::Error>;
        #[operation(source)]
        async fn infer_unpacked_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: Expression<'db>, unpack: Unpack<'db>, target: &ast::Expr, value: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_target(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: Expression<'db>, target: &ast::Expr, value: &ast::Expr) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl AssignmentStatementFacts {
        fn single_name<'stmt>(&self, assignment: &'stmt ast::StmtAssign) -> Option<&'stmt ast::ExprName> {
            match assignment.targets.as_slice() {
                [ast::Expr::Name(name)] => Some(name),
                _ => None,
            }
        }
        fn targets<'stmt>(&self, assignment: &'stmt ast::StmtAssign) -> &'stmt [ast::Expr] {
            &assignment.targets
        }
        fn value<'stmt>(&self, assignment: &'stmt ast::StmtAssign) -> &'stmt ast::Expr {
            &assignment.value
        }
        fn owns_value_bindings(&self, builder: &TypeInferenceBuilder<'_, '_>) -> bool {
            !matches!(builder.region, InferenceRegion::Scope(..))
        }
    }

    #[synchronous(infer_assignment_statement_sync)]
    #[capabilities(effects = AssignmentStatementEffects, facts = AssignmentStatementFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn infer_assignment_statement_with<'db, 'ast, E: AssignmentStatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &ast::StmtAssign,
        facts: AssignmentStatementFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        if let Some(name) = facts.single_name(assignment) {
            return effects.infer_name(builder, name).await;
        }

        let value = facts.value(assignment);
        let shared_value = effects.shared_value(builder, value).await?;

        if facts.owns_value_bindings(builder) {
            // The statement owns every binding created while evaluating its shared value,
            // including assignment expressions in lambda defaults.
            effects.retain_value_bindings(builder, shared_value).await?;
        }

        let mut cursor = 0;
        #[cursor_loop]
        while let Some(target) = effects.next_target(facts.targets(assignment), &mut cursor).await? {
            if let Some(unpack) = effects.unpack(builder, target).await? {
                effects.infer_unpacked_target(builder, shared_value, unpack, target, value).await?;
            } else if let ast::Expr::Name(name) = target {
                effects.infer_name(builder, name).await?;
            } else {
                effects.infer_target(builder, shared_value, target, value).await?;
            }
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousAssignmentStatementEffects<'db, 'ast>
    for OrdinaryAssignmentStatementEffects
{
    type Error = Infallible;

    fn infer_name(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: &ast::ExprName,
    ) -> Result<(), Self::Error> {
        builder.infer_definition(name);
        Ok(())
    }

    fn shared_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> Result<Expression<'db>, Self::Error> {
        Ok(builder.index.expression(value))
    }

    fn retain_value_bindings(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: Expression<'db>,
    ) -> Result<(), Self::Error> {
        let inference = infer_expression_types(builder.db(), expression, TypeContext::default());
        if let Some(extra) = &inference.extra {
            builder.bindings.extend(extra.bindings.iter().copied());
        }
        Ok(())
    }

    fn next_target<'target>(
        &self,
        targets: &'target [ast::Expr],
        cursor: &mut usize,
    ) -> Result<Option<&'target ast::Expr>, Self::Error> {
        let next = targets.get(*cursor);
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
    ) -> Result<Option<Unpack<'db>>, Self::Error> {
        Ok(builder.index.try_unpack(target))
    }

    fn infer_unpacked_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: Expression<'db>,
        unpack: Unpack<'db>,
        target: &ast::Expr,
        value: &ast::Expr,
    ) -> Result<(), Self::Error> {
        let inference = infer_expression_types(builder.db(), expression, TypeContext::default());
        builder.extend_expression_without_bindings(inference);

        let unpacked = infer_unpack_types(builder.db(), unpack);
        builder.context.extend(unpacked.diagnostics());
        builder.infer_unpacked_assignment_target(target, value, unpacked);
        Ok(())
    }

    fn infer_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: Expression<'db>,
        target: &ast::Expr,
        value: &ast::Expr,
    ) -> Result<(), Self::Error> {
        builder.infer_target(target, value, &|builder, tcx| {
            let inference = infer_expression_types(builder.db(), expression, tcx);
            builder.extend_expression_without_bindings(inference);
            inference.expression_type(value)
        });
        Ok(())
    }
}
