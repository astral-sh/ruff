use std::cmp::Ordering;

use num_bigint::{BigInt, Sign};
use ruff_macros::derive_message_formats;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located, Stmt};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;
use textwrap::{dedent, indent};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
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

#[derive(Debug)]
struct TokenCheck {
    first_token: Tok,
    has_elif: bool,
}

impl TokenCheck {
    fn new(first_token: Tok, has_elif: bool) -> Self {
        Self {
            first_token,
            has_elif,
        }
    }
}

fn check_tokens<T>(locator: &Locator, located: &Located<T>) -> TokenCheck {
    let text = locator.slice_source_code_range(&Range::from_located(located));
    let curr_indent = indentation(locator, located).unwrap();
    // I am worried this might cause issues in some situations, but im not sure what
    // those would be As far as I am concerned, the indent will always be cut
    // off
    let final_text = format!("{curr_indent}{text}");
    let tokens = lexer::make_tokenizer(&final_text);
    let mut first_token: Option<Tok> = None;
    let mut has_elif = false;

    for token_item in tokens {
        let token = token_item.unwrap().1;
        if first_token.is_none() && Tok::Indent != token {
            first_token = Some(token.clone());
        }
        if token == Tok::Elif {
            has_elif = true;
        }
        if let Some(first) = &first_token {
            if has_elif {
                return TokenCheck::new(first.clone(), has_elif);
            }
        }
    }
    TokenCheck::new(first_token.unwrap(), has_elif)
}

/// Converts a `BigInt` to a `u32`, if the number is negative, it will return 0
fn bigint_to_u32(number: &BigInt) -> u32 {
    let the_number = number.to_u32_digits();
    match the_number.0 {
        Sign::Minus | Sign::NoSign => 0,
        Sign::Plus => *the_number.1.first().unwrap(),
    }
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
            let number = bigint_to_u32(item);
            version.push(number);
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

fn indent_after_first(text: &str, indent_str: &str) -> String {
    let first_newline = match text.find('\n') {
        Some(item) => item,
        None => return text.to_string(),
    };
    if text.len() < first_newline + 2 {
        return text.to_string();
    }
    let first_part = &text[..=first_newline];
    let second_part = &text[first_newline + 1..];
    let new_second_part = indent(second_part, indent_str);
    let mut final_string = String::from(first_part);
    final_string.push_str(&new_second_part);
    final_string
}

/// Converts an if statement where the code to keep is in the else statement
fn fix_py2_block(
    checker: &mut Checker,
    stmt: &Stmt,
    body: &[Stmt],
    orelse: &[Stmt],
    tokens: &TokenCheck,
) {
    if orelse.is_empty() {
        return;
    }
    // FOR REVIEWER: pyupgrade had a check to see if the first statement was an if
    // or an elif, and would check for an index based on this. Our parser
    // automatically only sends the start of the statement as the if or elif, so
    // I did not see that as necessary.
    let else_statement = orelse.last().unwrap();
    let mut ending_location = else_statement.location;
    let range = Range::new(stmt.location, stmt.end_location.unwrap());
    let mut diagnostic = Diagnostic::new(OldCodeBlocks, range);
    // If we only have an if and an else, we just need to get the else code and
    // dedent
    if tokens.first_token == Tok::If && !tokens.has_elif {
        let start = orelse.first().unwrap();
        let end = orelse.last().unwrap();
        let the_range = Range::new(start.location, end.end_location.unwrap());
        let text = checker.locator.slice_source_code_range(&the_range);
        let curr_indent = indentation(checker.locator, start).unwrap_or_default();
        // Includes the first indent so thag we can properly dedent the string
        let whole_text = format!("{curr_indent}{text}");
        let closer_text = dedent(&whole_text);
        let if_indent = indentation(checker.locator, stmt).unwrap_or_default();
        // The replacement library only remembers the correct indentation for the first
        // line anything after that, we need to run indent manually
        let final_text = indent_after_first(&closer_text, if_indent);
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                final_text,
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
        return;
    // If we have an elif, we need the "e" and "l" to make an if
    } else if tokens.first_token == Tok::If && tokens.has_elif {
        ending_location.go_right();
        ending_location.go_right();
    } else if tokens.first_token == Tok::Elif {
        ending_location = body.last().unwrap().end_location.unwrap();
        let mut next_item = ending_location;
        next_item.go_right();
        let mut after_item = ending_location;
        after_item.go_right();
        let range = Range::new(ending_location, after_item);
        let next_str = checker.locator.slice_source_code_range(&range);
        // If the next item it a new line, remove it so that we do not get an empty line
        if next_str == "\n" {
            ending_location.go_right();
        }
    }
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::deletion(stmt.location, ending_location));
    }
    checker.diagnostics.push(diagnostic);
}

/// This is called to fix statements where the code to keep is not in the else
fn fix_py3_block(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    tokens: &TokenCheck,
) {
    let mut new_text: String;
    // If the first statement is an if, just use the body of this statement, and the
    // rest of the statement can be ignored, because we basically have `if True:`
    if tokens.first_token == Tok::If {
        if body.is_empty() {
            return;
        }
        if tokens.has_elif {
            let start = body.first().unwrap();
            let end = body.last().unwrap();
            let text_range = Range::new(start.location, end.end_location.unwrap());
            new_text = checker
                .locator
                .slice_source_code_range(&text_range)
                .to_string();
        } else {
            let start = body.first().unwrap();
            let end = body.last().unwrap();
            let the_range = Range::new(start.location, end.end_location.unwrap());
            let text = checker.locator.slice_source_code_range(&the_range);
            let curr_indent = indentation(checker.locator, start).unwrap_or_default();
            // Includes the first indent so thag we can properly dedent the string
            let whole_text = format!("{curr_indent}{text}");
            let closer_text = dedent(&whole_text);
            let if_indent = indentation(checker.locator, stmt).unwrap_or_default();
            // The replacement library only remembers the correct indentation for the first
            // line anything after that, we need to run indent manually
            new_text = indent_after_first(&closer_text, if_indent);
        }
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
    let tokens = check_tokens(checker.locator, stmt);
    match &test.node {
        ExprKind::Compare {
            left,
            ops,
            comparators,
        } => {
            if check_path(checker, left, &["sys", "version_info"]) {
                // We need to ensure we have only one operation and one comparison
                if ops.len() == 1 && comparators.len() == 1 {
                    let comparison = &comparators.get(0).unwrap().node;
                    let op = ops.get(0).unwrap();
                    match comparison {
                        ExprKind::Tuple { elts, .. } => {
                            // Here we check for the correct operator, and also adjust the desired
                            // target based on whether we are accepting equal to
                            let version = extract_version(elts);
                            let target = checker.settings.target_version;
                            if op == &Cmpop::Lt || op == &Cmpop::LtE {
                                if compare_version(&version, target, op == &Cmpop::LtE) {
                                    fix_py2_block(checker, stmt, body, orelse, &tokens);
                                }
                            } else if op == &Cmpop::Gt || op == &Cmpop::GtE {
                                if compare_version(&version, target, op == &Cmpop::GtE) {
                                    fix_py3_block(checker, stmt, test, body, &tokens);
                                }
                            }
                        }
                        ExprKind::Constant {
                            value: Constant::Int(number),
                            ..
                        } => {
                            let version_number = bigint_to_u32(number);
                            if version_number == 2 && op == &Cmpop::Eq {
                                fix_py2_block(checker, stmt, body, orelse, &tokens);
                            } else if version_number == 3 && op == &Cmpop::Eq {
                                fix_py3_block(checker, stmt, test, body, &tokens);
                            }
                        }
                        _ => (),
                    }
                }
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
