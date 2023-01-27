use num_bigint::Sign;
use ruff_macros::derive_message_formats;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located, Stmt, Unaryop};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::{Range, RefEquality};
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::settings::types::PythonVersion;
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct OldCodeBlocks;
);
impl AlwaysAutofixableViolation for OldCodeBlocks {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove old code blocks")
    }

    fn autofix_title(&self) -> String {
        "Rewrite to only contain new block".to_string()
    }
}

/// Checks whether the give attribute is from the given path
fn check_path(checker: &Checker, expr: &Expr, path: &[&str]) -> bool {
    checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == path)
}

struct TokenCheck {
    first_token: Tok,
    has_else: bool,
    has_elif: bool,
}

impl TokenCheck {
    fn new(first_token: Tok, has_else: bool, has_elif: bool) -> Self {
        Self {
            first_token,
            has_else,
            has_elif,
        }
    }
}

/// Checks whether the parent statement is an if statement
fn check_parent_if(checker: &Checker, stmt: &Stmt) -> bool {
    let parent = match checker.child_to_parent.get(&RefEquality(stmt)) {
        Some(parent) => parent,
        None => return false,
    };
    let text = checker
        .locator
        .slice_source_code_range(&Range::from_located(&parent));
    let mut tokens = lexer::make_tokenizer(&text);
    tokens.next().unwrap().unwrap().1 == Tok::If
}

/// Checks for a single else in the statement provided
fn check_tokens<T>(locator: &Locator, located: &Located<T>) -> TokenCheck {
    let text = locator.slice_source_code_range(&Range::from_located(&located));
    let mut tokens = lexer::make_tokenizer(&text);
    let first_token = tokens.next().unwrap().unwrap().1;
    let has_else = tokens
        .by_ref()
        .map(|token| token.unwrap().1 == Tok::Else)
        .any(|x| x);
    let has_elif = tokens
        .by_ref()
        .map(|token| token.unwrap().1 == Tok::Elif)
        .any(|x| x);
    TokenCheck::new(first_token, has_else, has_elif)
}

/// Gets the version from the tuple
fn extract_version(elts: &[Expr]) -> Vec<u32> {
    let mut version: Vec<u32> = vec![];
    for elt in elts {
        if let ExprKind::Constant {
            value: Constant::Int(item),
            ..
        } = &elt.node
        {
            let the_number = item.to_u32_digits();
            match the_number.0 {
                // We do not have a way of handling these values, so return what was gathered
                Sign::Minus | Sign::NoSign => {
                    return version;
                }
                Sign::Plus => {
                    // Assuming that the version will never be above a 32 bit
                    version.push(*the_number.1.get(0).unwrap())
                }
            }
        } else {
            return version;
        }
    }
    version
}

/// Returns true if the if_version is less than the PythonVersion
fn compare_version_lt(if_version: Vec<u32>, py_version: PythonVersion, or_equal: bool) -> bool {
    let mut ver_iter = if_version.iter();
    // Check the first number (the major version)
    if let Some(first) = ver_iter.next() {
        if *first < 3 {
            return true;
        } else if *first == 3 {
            // Check the second number (the minor version)
            if let Some(second) = ver_iter.next() {
                // If there is an equal, then we need to require one level higher of python
                if *second < py_version.to_tuple().1 + or_equal as u32 {
                    return true;
                }
            } else {
                // If there is no second number was assumed python 3.0, and upgrade
                return true;
            }
        }
    }
    false
}

/// Returns true if the if_version is greater than the PythonVersion
fn compare_version_gt(if_version: Vec<u32>, py_version: PythonVersion, or_equal: bool) -> bool {
    println!("if_version: {:?}", if_version);
    println!("py_version: {:?}", py_version);
    let mut ver_iter = if_version.iter();
    // Check the first number (the major version)
    if let Some(first) = ver_iter.next() {
        if *first < 3 {
            return false;
        } else if *first == 3 {
            println!("Not less than 3");
            // Check the second number (the minor version)
            if let Some(second) = ver_iter.next() {
                println!("Not less than 3 next");
                // If there is an equal, then we need to require one level higher of python
                if *second + or_equal as u32 > py_version.to_tuple().1 {
                    return true;
                }
            } else {
                // If there is no second number was assumed python 3.0, and upgrade
                return false;
            }
        }
    }
    false
}

/// Converts an if statement that has the py2 block on top
fn fix_py2_block(checker: &mut Checker, stmt: &Stmt, orelse: &[Stmt]) {
    // FOR REVIEWER: pyupgrade had a check to see if the first statement was an if
    // or an elif, and would check for an index based on this. Our parser
    // automatically only sends the start of the statement as the if or elif, so
    // I did not see that as necessary.
    let token_checker = check_tokens(checker.locator, stmt);
    let has_else = token_checker.has_else;
    // The statement MUST have an else
    if !has_else {
        return;
    }
    let else_statement = orelse.last().unwrap();
    let mut ending_location = else_statement.location;
    // If we have an elif, we need the "e" and "l" to make an if
    if token_checker.first_token == Tok::If && token_checker.has_elif {
        ending_location.go_right();
        ending_location.go_right();
    }
    let text = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    println!("\n{}\n", text);
    let range = Range::new(stmt.location, ending_location);
    let mut diagnostic = Diagnostic::new(OldCodeBlocks, range);
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::deletion(stmt.location, ending_location));
    }
    checker.diagnostics.push(diagnostic);
}

/// UP037
pub fn old_code_blocks(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
) {
    // NOTE: Pyupgrade ONLY works if `sys.version_info` is on the left
    // We have to have an else statement in order to refactor
    println!("====================");
    if orelse.is_empty() {
        return;
    }
    match &test.node {
        ExprKind::Compare {
            left,
            ops,
            comparators,
        } => {
            if check_path(checker, left, &["sys", "version_info"]) {
                // We need to ensure we have only one operation and one comparison
                if ops.len() == 1 && comparators.len() == 1 {
                    // DO NOT forget to check for LT or LTE
                    if let ExprKind::Tuple { elts, .. } = &comparators.get(0).unwrap().node {
                        let op = ops.get(0).unwrap();
                        // Here we check for the correct operator, and also adjust the desired
                        // target based on whether we are accepting equal to
                        if op == &Cmpop::Lt || op == &Cmpop::LtE {
                            if compare_version_lt(
                                extract_version(elts),
                                checker.settings.target_version,
                                op == &Cmpop::LtE,
                            ) {
                                fix_py2_block(checker, stmt, orelse);
                            }
                        } else if op == &Cmpop::Gt || op == &Cmpop::GtE {
                            if compare_version_gt(
                                extract_version(elts),
                                checker.settings.target_version,
                                op == &Cmpop::LtE,
                            ) {
                                fix_py2_block(checker, stmt, orelse);
                            }
                        }
                    }
                }
            }
        }
        ExprKind::Attribute { .. } => {
            // if six.PY2
            if check_path(checker, test, &["six", "PY2"]) {
                fix_py2_block(checker, stmt, orelse);
            }
        }
        ExprKind::UnaryOp { op, operand } => {
            // if not six.PY3
            if check_path(checker, operand, &["six", "PY3"]) && op == &Unaryop::Not {
                fix_py2_block(checker, stmt, orelse);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::*;

    #[test_case(PythonVersion::Py37, vec![2], true, true; "compare-2.0")]
    #[test_case(PythonVersion::Py37, vec![2, 0], true, true; "compare-2.0-whole")]
    #[test_case(PythonVersion::Py37, vec![3], true, true; "compare-3.0")]
    #[test_case(PythonVersion::Py37, vec![3, 0], true, true; "compare-3.0-whole")]
    #[test_case(PythonVersion::Py37, vec![3, 1], true, true; "compare-3.1")]
    #[test_case(PythonVersion::Py37, vec![3, 5], true, true; "compare-3.5")]
    #[test_case(PythonVersion::Py37, vec![3, 7], true, true; "compare-3.7")]
    #[test_case(PythonVersion::Py37, vec![3, 7], false, false; "compare-3.7-not-equal")]
    #[test_case(PythonVersion::Py37, vec![3, 8], false , false; "compare-3.8")]
    #[test_case(PythonVersion::Py310, vec![3,9], true, true; "compare-3.9")]
    #[test_case(PythonVersion::Py310, vec![3, 11], true, false; "compare-3.11")]
    fn test_compare_version_lt(
        version: PythonVersion,
        version_vec: Vec<u32>,
        or_equal: bool,
        expected: bool,
    ) {
        assert_eq!(compare_version_lt(version_vec, version, or_equal), expected);
    }
    #[test_case(PythonVersion::Py37, vec![2], true, false; "compare-2.0")]
    #[test_case(PythonVersion::Py37, vec![2, 0], true, false; "compare-2.0-whole")]
    #[test_case(PythonVersion::Py37, vec![3], true, false; "compare-3.0")]
    #[test_case(PythonVersion::Py37, vec![3, 0], true, false; "compare-3.0-whole")]
    #[test_case(PythonVersion::Py37, vec![3, 1], true, false; "compare-3.1")]
    #[test_case(PythonVersion::Py37, vec![3, 5], true, false; "compare-3.5")]
    #[test_case(PythonVersion::Py37, vec![3, 7], true, true; "compare-3.7")]
    #[test_case(PythonVersion::Py37, vec![3, 7], false, false; "compare-3.7-not-equal")]
    #[test_case(PythonVersion::Py37, vec![3, 8], false , true; "compare-3.8")]
    #[test_case(PythonVersion::Py310, vec![3,9], true, false; "compare-3.9")]
    #[test_case(PythonVersion::Py310, vec![3, 11], true, true; "compare-3.11")]
    fn test_compare_version_gt(
        version: PythonVersion,
        version_vec: Vec<u32>,
        or_equal: bool,
        expected: bool,
    ) {
        assert_eq!(compare_version_gt(version_vec, version, or_equal), expected);
    }
}
