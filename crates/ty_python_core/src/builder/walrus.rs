use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};

use crate::scope::{NodeWithScopeKind, ScopeKind};

use super::SemanticIndexBuilder;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WalrusTargetScope {
    Current,
    Enclosing,
    InvalidClassBodyComprehension,
}

impl<'ast> SemanticIndexBuilder<'_, 'ast> {
    pub(super) fn visit_named_expression(&mut self, node: &'ast ast::ExprNamed) {
        self.visit_expr(&node.value);

        // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
        if node.target.is_name_expr() {
            let walrus_target_scope = self.walrus_target_scope();
            let invalid_in_comprehension_iterable = self.in_comprehension_iterable();
            let target = node
                .target
                .as_name_expr()
                .expect("named expression target was checked as a name");
            let rebinds_comprehension_variable = !invalid_in_comprehension_iterable
                && self.rebinds_comprehension_variable(&target.id);

            if invalid_in_comprehension_iterable {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInComprehensionIterable,
                    range: node.range,
                    python_version: self.python_version,
                });
            } else if !rebinds_comprehension_variable
                && walrus_target_scope == WalrusTargetScope::InvalidClassBodyComprehension
            {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInClassBodyComprehension,
                    range: node.range,
                    python_version: self.python_version,
                });
            }

            self.push_assignment(node.into());
            self.visit_expr(&node.target);
            self.pop_assignment();
        } else {
            self.visit_expr(&node.target);
        }
    }

    fn walrus_target_scope(&self) -> WalrusTargetScope {
        if self.scopes[self.current_scope()].kind() != ScopeKind::Comprehension {
            return WalrusTargetScope::Current;
        }

        self.scope_stack
            .iter()
            .rev()
            .skip(1)
            .find_map(|info| match self.scopes[info.file_scope_id].kind() {
                ScopeKind::Comprehension => None,
                ScopeKind::Class => Some(WalrusTargetScope::InvalidClassBodyComprehension),
                _ => Some(WalrusTargetScope::Enclosing),
            })
            .unwrap_or(WalrusTargetScope::Current)
    }

    /// Returns whether a named expression evaluated in the current comprehension chain
    /// rebinds one of that chain's iteration variables.
    fn rebinds_comprehension_variable(&self, name: &ast::name::Name) -> bool {
        self.scope_stack
            .iter()
            .rev()
            .map(|info| &self.scopes[info.file_scope_id])
            .take_while(|scope| scope.kind() == ScopeKind::Comprehension)
            .any(|scope| {
                let generators: &[ast::Comprehension] = match scope.node() {
                    NodeWithScopeKind::ListComprehension(node) => {
                        &node.node(self.module).generators
                    }
                    NodeWithScopeKind::SetComprehension(node) => &node.node(self.module).generators,
                    NodeWithScopeKind::DictComprehension(node) => {
                        &node.node(self.module).generators
                    }
                    NodeWithScopeKind::GeneratorExpression(node) => {
                        &node.node(self.module).generators
                    }
                    _ => unreachable!("comprehension scope should have a comprehension node"),
                };

                generators
                    .iter()
                    .any(|generator| target_binds_name(&generator.target, name))
            })
    }
}

fn target_binds_name(target: &ast::Expr, name: &ast::name::Name) -> bool {
    match target {
        ast::Expr::Name(target) => &target.id == name,
        ast::Expr::Tuple(target) => target
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        ast::Expr::List(target) => target
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        ast::Expr::Starred(target) => target_binds_name(&target.value, name),
        _ => false,
    }
}
