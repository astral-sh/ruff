use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};

use rustc_hash::FxHashSet;

use crate::scope::ScopeKind;

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

            if !invalid_in_comprehension_iterable
                && walrus_target_scope == WalrusTargetScope::InvalidClassBodyComprehension
                && !self.rebinds_active_comprehension_variable(&target.id)
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

    pub(super) fn mark_comprehension_target_active(&mut self, target: &ast::Expr) {
        let active_targets = self
            .active_comprehension_targets
            .last_mut()
            .expect("comprehension target state should match comprehension scopes");
        add_target_names(target, active_targets);
    }

    /// Returns whether a named expression rebinds an iteration variable that is active at this
    /// point in the current comprehension chain.
    fn rebinds_active_comprehension_variable(&self, name: &ast::name::Name) -> bool {
        self.active_comprehension_targets
            .iter()
            .rev()
            .any(|targets| targets.contains(name))
    }
}

fn add_target_names(target: &ast::Expr, names: &mut FxHashSet<ast::name::Name>) {
    match target {
        ast::Expr::Name(target) => {
            names.insert(target.id.clone());
        }
        ast::Expr::Tuple(target) => {
            for element in &target.elts {
                add_target_names(element, names);
            }
        }
        ast::Expr::List(target) => {
            for element in &target.elts {
                add_target_names(element, names);
            }
        }
        ast::Expr::Starred(target) => add_target_names(&target.value, names),
        _ => {}
    }
}
