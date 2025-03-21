use ruff_python_ast::{self as ast, Expr, ExprContext, ExprName, Pattern};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pep8_naming;
use ruff_python_ast::Identifier;
use ruff_python_semantic::ScopeKind;

/// Pattern-level analysis for pattern matching constructs.
///
/// Traverses the pattern to detect any variable bindings and runs
/// name style checks (e.g., PEP 8 naming conventions) against those
/// bindings depending on the current scope.
pub(crate) fn pattern(pattern: &Pattern, checker: &Checker) {
    match pattern {
        // `case <name>`
        Pattern::MatchAs(ast::PatternMatchAs { name: Some(name), .. })
        // `case *<name>`
        | Pattern::MatchStar(ast::PatternMatchStar { name: Some(name), range: _ })
        // `case **<name>`
        | Pattern::MatchMapping(ast::PatternMatchMapping { rest: Some(name), .. }) => {
            check_pattern_variable(checker, name);
        }
        _ => {}
    }
}

/// Run the appropriate pep8-naming rules for a pattern variable binding.
fn check_pattern_variable(checker: &Checker, name: &Identifier) {
    let id = name.as_str();
    // Construct a temporary `Expr::Name` node to point at the identifier.
    let temp_expr = Expr::Name(ExprName {
        range: name.range(),
        id: name.clone().into(),
        ctx: ExprContext::Store,
    });
    // Determine which naming rule applies based on the current scope.
    let current = &checker.semantic().current_scope().kind;
    if current.is_function() {
        if checker.enabled(Rule::NonLowercaseVariableInFunction) {
            pep8_naming::rules::non_lowercase_variable_in_function(checker, &temp_expr, id);
        }
    }
    if let ScopeKind::Class(class_def) = current {
        if checker.enabled(Rule::MixedCaseVariableInClassScope) {
            pep8_naming::rules::mixed_case_variable_in_class_scope(
                checker, &temp_expr, id, class_def,
            );
        }
    }
    if matches!(current, ScopeKind::Module) {
        if checker.enabled(Rule::MixedCaseVariableInGlobalScope) {
            pep8_naming::rules::mixed_case_variable_in_global_scope(checker, &temp_expr, id);
        }
    }
}
