use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::ast::whitespace::indentation_greedy;
use crate::violations;
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::lexer::{self, Tok};
use std::cmp::max;

/// A boolean of whether or not an expression has more than one set of parenthesis. Please note
/// there are other factors besides this for a function to be extraneous.
fn valid_candidate(string: &str) -> bool {
    let mut depth = 0;
    let mut max_depth = 0;
    for (_, tok, _) in lexer::make_tokenizer(string).flatten() {
        match tok {
            Tok::Lpar => {
                depth += 1;
                max_depth = max(depth, max_depth);
            }
            Tok::Rpar => depth -= 1,
            Tok::Comma | Tok::Yield if depth == 1 => return false,
            _ => (),
        }
    }
    max_depth > 1
}

/// UP033
pub fn extraneous_parenthesis(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
) {
    // If the function has kwargs, we won't be refactoring
    if !kwargs.is_empty() {
        return;
    }
    // If the function has more than one argument, or no arguments, we won't be refactoring
    if args.len() != 1 {
        return;
    }
    if let ExprKind::Name { id, .. } = &func.node {
        // This fixer is only for print statements
        if id != "print" {
            return;
        }
    } else {
        return;
    }
    let arg = match args.get(0) {
        None => return,
        Some(arg) => arg,
    };
    // Only go if we are dealing with a constant
    let mut new_string = String::new();
    let expr_range = Range::from_located(expr);
    let expr_string = checker
        .locator
        .slice_source_code_range(&expr_range)
        .to_string();
    let is_multi_line = expr_string.contains('\n');
    if valid_candidate(&expr_string) {
        let arg_range = Range::from_located(arg);
        let arg_string = checker.locator.slice_source_code_range(&arg_range);
        // This is a little aggressive, but I think I added sufficient checks to make sure it
        // should happen
        if is_multi_line {
            let indent = indentation_greedy(checker.locator, &arg);
            let small_indent = if indent.len() > 3 {
                &indent[3..]
            } else {
                ""
            };
            new_string = format!("print(\n{indent}{arg_string}\n{small_indent})");
        } else {
            new_string = format!("print({arg_string})");
        }
    }
    if !new_string.is_empty() && new_string != expr_string {
        let mut diagnostic = Diagnostic::new(violations::ExtraneousParenthesis, expr_range);
        if checker.patch(diagnostic.kind.code()) {
            diagnostic.amend(Fix::replacement(
                new_string.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        };
        checker.diagnostics.push(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("print(1)", false ; "basic print")]
    #[test_case("print(\"hello world\")", false ; "print a string")]
    #[test_case("print(\"hello((goodybe)) world\")", false ; "inside string")]
    #[test_case("print((1),)", false ; "a tuple")]
    #[test_case("print(yield (1))", false ; "a yield")]
    #[test_case("print((((1),)))", false ; "nested tuple")]
    #[test_case("print((yield ((1),)))", false ; "nested yield")]
    #[test_case("print((1))", true ; "basic positive example")]
    #[test_case("print((\"hello world\"))", true ; "print a positive string")]
    fn test_valid_candidate(string: &str, expected: bool) {
        assert_eq!(valid_candidate(string), expected);
    }
}
