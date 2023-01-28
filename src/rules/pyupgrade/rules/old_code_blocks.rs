use std::cmp::Ordering;

use num_bigint::Sign;
use ruff_macros::derive_message_formats;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located, Stmt, Unaryop};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
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

/// Checks for a single else in the statement provided
fn check_tokens<T>(locator: &Locator, located: &Located<T>) -> TokenCheck {
    let text = locator.slice_source_code_range(&Range::from_located(located));
    let tokens = lexer::make_tokenizer(text);
    let mut first_token: Option<Tok> = None;
    let mut has_else = false;
    let mut has_elif = false;

    for token_item in tokens {
        let token = token_item.unwrap().1;
        if first_token.is_none() {
            first_token = Some(token.clone());
        }
        if token == Tok::Else {
            has_else = true;
        } else if token == Tok::Elif {
            has_elif = true;
        }
        if has_else && has_elif {
            return TokenCheck::new(first_token.unwrap(), has_else, has_elif);
        }
    }
    TokenCheck::new(first_token.unwrap(), has_else, has_elif)
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
                    version.push(*the_number.1.first().unwrap());
                }
            }
        } else {
            return version;
        }
    }
    version
}

/// Returns true if the `if_version` is less than the `PythonVersion`
fn compare_version(if_version: &[u32], py_version: PythonVersion, or_equal: bool) -> bool {
    let mut ver_iter = if_version.iter();
    // Check the first number (the major version)
    if let Some(first) = ver_iter.next() {
        match first.cmp(&3) {
            Ordering::Less => return true,
            Ordering::Equal => {
                if let Some(second) = ver_iter.next() {
                    // Check the second number (the minor version)
                    // If there is an equal, then we need to require one level higher of python
                    return *second < py_version.to_tuple().1 + u32::from(or_equal);
                }
                // If there is no second number was assumed python 3.0, and upgrade
                return true;
            }
            Ordering::Greater => return false,
        }
    }
    false
}

/// Converts an if statement where the code to keep is in the else statement
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
    let range = Range::new(stmt.location, stmt.end_location.unwrap());
    let mut diagnostic = Diagnostic::new(OldCodeBlocks, range);
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::deletion(stmt.location, ending_location));
    }
    checker.diagnostics.push(diagnostic);
}

/// This is called to fix statements where the code to keep is not in the else
fn fix_py3_block(checker: &mut Checker, stmt: &Stmt, test: &Expr, body: &[Stmt]) {
    println!("Checkpoint three");
    let token_checker = check_tokens(checker.locator, stmt);
    let mut new_text: String;
    // If the first statement is an if, just use the body of this statement, and the
    // rest of the statement can be ignored, because we basically have `if True:`
    if token_checker.first_token == Tok::If {
        if body.is_empty() {
            return;
        }
        let start = body.first().unwrap();
        let end = body.last().unwrap();
        let text_range = Range::new(start.location, end.end_location.unwrap());
        new_text = checker
            .locator
            .slice_source_code_range(&text_range)
            .to_string();
    // Here we are dealing with an elif, so we need to replace it with an else,
    // preserve the body of the elif, and remove anything else
    } else {
        let end = body.last().unwrap();
        let text_range = Range::new(stmt.location, end.end_location.unwrap());
        let test_range = Range::from_located(test);
        let whole_text = checker.locator.slice_source_code_range(&text_range);
        let new_test = checker.locator.slice_source_code_range(&test_range);
        // First we remove the test text, so that if there is a colon the the test it
        // doesn't cause issues
        let clean1 = whole_text.replace(new_test, "");
        let colon_index = clean1.find(':').unwrap();
        let clean2 = &clean1[colon_index..];
        new_text = String::from("else");
        new_text.push_str(clean2);
    }
    let mut diagnostic = Diagnostic::new(OldCodeBlocks, Range::from_located(stmt));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            new_text,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
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
                    if let ExprKind::Tuple { elts, .. } = &comparators.get(0).unwrap().node {
                        let op = ops.get(0).unwrap();
                        // Here we check for the correct operator, and also adjust the desired
                        // target based on whether we are accepting equal to
                        let version = extract_version(elts);
                        let target = checker.settings.target_version;
                        if op == &Cmpop::Lt || op == &Cmpop::LtE {
                            if compare_version(&version, target, op == &Cmpop::LtE) {
                                fix_py2_block(checker, stmt, orelse);
                            }
                        } else if op == &Cmpop::Gt || op == &Cmpop::GtE {
                            if compare_version(&version, target, op == &Cmpop::GtE) {
                                fix_py3_block(checker, stmt, test, body);
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

    #[test_case(PythonVersion::Py37, &[2], true, true; "compare-2.0")]
    #[test_case(PythonVersion::Py37, &[2, 0], true, true; "compare-2.0-whole")]
    #[test_case(PythonVersion::Py37, &[3], true, true; "compare-3.0")]
    #[test_case(PythonVersion::Py37, &[3, 0], true, true; "compare-3.0-whole")]
    #[test_case(PythonVersion::Py37, &[3, 1], true, true; "compare-3.1")]
    #[test_case(PythonVersion::Py37, &[3, 5], true, true; "compare-3.5")]
    #[test_case(PythonVersion::Py37, &[3, 7], true, true; "compare-3.7")]
    #[test_case(PythonVersion::Py37, &[3, 7], false, false; "compare-3.7-not-equal")]
    #[test_case(PythonVersion::Py37, &[3, 8], false , false; "compare-3.8")]
    #[test_case(PythonVersion::Py310, &[3,9], true, true; "compare-3.9")]
    #[test_case(PythonVersion::Py310, &[3, 11], true, false; "compare-3.11")]
    fn test_compare_version(
        version: PythonVersion,
        version_vec: &[u32],
        or_equal: bool,
        expected: bool,
    ) {
        assert_eq!(compare_version(version_vec, version, or_equal), expected);
    }
}
