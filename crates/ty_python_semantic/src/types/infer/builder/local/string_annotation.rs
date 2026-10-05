//! Quoted type expressions borrow parsed syntax until their local invocation has retired.

use ruff_python_ast as ast;
use ruff_python_parser::Parsed;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::node_key::NodeKey;

use super::super::type_expression::{TypeExpressionMode, TypeExpressionRequest};
use super::super::{DeferredExpressionState, InferenceFlags, TypeInferenceBuilder};
use crate::types::infer::TypeExpressionFlags;

pub(super) type ParsedAnnotation = Parsed<ast::ModExpression>;
pub(super) type OrdinaryStorage = std::cell::OnceCell<typed_arena::Arena<ParsedAnnotation>>;

/// Restores the enclosing expression after the parsed quoted expression returns.
///
/// The parsed root uses the outermost string's lookup context. Clearing the outer
/// type-expression flag keeps the parsed root at the same nesting level as the string.
#[derive(Clone, Copy, Debug)]
pub(super) struct Scope<'expr> {
    pub(super) string: &'expr ast::ExprStringLiteral,
    pub(super) parsed: &'expr ast::Expr,
    pub(super) enclosing: NodeKey,
    in_type_expression: bool,
    in_nested_type_expression: bool,
}

impl<'expr> Scope<'expr> {
    /// Saves the string's nesting state and the deferred context of its parsed child.
    pub(super) fn prepare(
        builder: &TypeInferenceBuilder<'_, '_>,
        string: &'expr ast::ExprStringLiteral,
        parsed: &'expr ast::Expr,
    ) -> Self {
        Self {
            string,
            parsed,
            enclosing: builder.enclosing_node_key(ast::AnyNodeRef::from(string)),
            in_type_expression: builder
                .inference_flags()
                .contains(InferenceFlags::IN_TYPE_EXPRESSION),
            in_nested_type_expression: builder
                .inference_flags()
                .contains(InferenceFlags::IN_NESTED_TYPE_EXPRESSION),
        }
    }

    /// Enters the parsed root at the string's existing type-expression nesting level.
    pub(super) fn enter(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_TYPE_EXPRESSION, false);
        builder.context.inference_flags.set(
            InferenceFlags::IN_NESTED_TYPE_EXPRESSION,
            self.in_nested_type_expression,
        );
    }

    pub(super) const fn request<'db>(self) -> TypeExpressionRequest<'db, 'expr> {
        TypeExpressionRequest::Expression {
            expression: self.parsed,
            mode: TypeExpressionMode::ScopedWithState(DeferredExpressionState::InStringAnnotation(
                self.enclosing,
            )),
        }
    }

    /// Restores outer nesting before the parsed root's flags are copied to the string.
    pub(super) fn restore(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder.context.inference_flags.set(
            InferenceFlags::IN_NESTED_TYPE_EXPRESSION,
            self.in_nested_type_expression,
        );
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_TYPE_EXPRESSION, self.in_type_expression);
    }

    pub(super) fn original_key(self) -> ExpressionNodeKey {
        ExpressionNodeKey::from(ast::ExprRef::StringLiteral(self.string))
    }

    pub(super) fn parsed_flags(
        self,
        builder: &TypeInferenceBuilder<'_, '_>,
    ) -> TypeExpressionFlags {
        builder.type_expression_flags(self.parsed)
    }

    pub(super) fn store_flags(
        self,
        builder: &mut TypeInferenceBuilder<'_, '_>,
        flags: TypeExpressionFlags,
    ) {
        builder.store_type_expression_flags(ast::ExprRef::StringLiteral(self.string), flags);
    }
}
