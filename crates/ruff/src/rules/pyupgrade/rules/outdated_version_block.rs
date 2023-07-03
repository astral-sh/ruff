use std::cmp::Ordering;

use num_bigint::{BigInt, Sign};
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::whitespace::indentation;

use crate::autofix::edits::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pyupgrade::fixes::adjust_indentation;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for conditional blocks gated on `sys.version_info` comparisons
/// that are outdated for the minimum supported Python version.
///
/// ## Why is this bad?
/// In Python, code can be conditionally executed based on the active
/// Python version by comparing against the `sys.version_info` tuple.
///
/// If a code block is only executed for Python versions older than the
/// minimum supported version, it should be removed.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info < (3, 0):
///     print("py2")
/// else:
///     print("py3")
/// ```
///
/// Use instead:
/// ```python
/// print("py3")
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct OutdatedVersionBlock;

impl AlwaysAutofixableViolation for OutdatedVersionBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Version block is outdated for minimum Python version")
    }

    fn autofix_title(&self) -> String {
        "Remove outdated version block".to_string()
    }
}

/// The metadata for a version-comparison block.
#[derive(Debug)]
struct BlockMetadata {
    /// The first `if` or `elif` token in the block, used to signal the start of the
    /// version-comparison block.
    leading_token: StartToken,
    /// The first `elif` or `else` token following the start token, if any, used to signal the end
    /// of the version-comparison block.
    trailing_token: Option<EndToken>,
}

/// The set of tokens that can start a block, i.e., the first token in an `if` statement.
#[derive(Debug)]
enum StartTok {
    If,
    Elif,
}

impl StartTok {
    fn from_tok(tok: &Tok) -> Option<Self> {
        match tok {
            Tok::If => Some(Self::If),
            Tok::Elif => Some(Self::Elif),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct StartToken {
    tok: StartTok,
    range: TextRange,
}

/// The set of tokens that can end a block, i.e., the first token in the subsequent `elif` or `else`
/// branch that follows an `if` or `elif` statement.
#[derive(Debug)]
enum EndTok {
    Elif,
    Else,
}

impl EndTok {
    fn from_tok(tok: &Tok) -> Option<Self> {
        match tok {
            Tok::Elif => Some(Self::Elif),
            Tok::Else => Some(Self::Else),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct EndToken {
    tok: EndTok,
    range: TextRange,
}

fn metadata<T>(locator: &Locator, located: &T, body: &[Stmt]) -> Option<BlockMetadata>
where
    T: Ranged,
{
    indentation(locator, located)?;

    let mut iter = lexer::lex_starts_at(
        locator.slice(located.range()),
        Mode::Module,
        located.start(),
    )
    .flatten();

    // First the leading `if` or `elif` token.
    let (tok, range) = iter.next()?;
    let leading_token = StartToken {
        tok: StartTok::from_tok(&tok)?,
        range,
    };

    // Skip any tokens until we reach the end of the `if` body.
    let body_end = body.last()?.range().end();

    // Find the trailing `elif` or `else` token, if any.
    let trailing_token = iter
        .skip_while(|(_, range)| range.start() < body_end)
        .find_map(|(tok, range)| EndTok::from_tok(&tok).map(|tok| EndToken { tok, range }));

    Some(BlockMetadata {
        leading_token,
        trailing_token,
    })
}

/// Converts a `BigInt` to a `u32`. If the number is negative, it will return 0.
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
        if let Expr::Constant(ast::ExprConstant {
            value: Constant::Int(item),
            ..
        }) = &elt
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
    let mut if_version_iter = if_version.iter();
    if let Some(if_major) = if_version_iter.next() {
        let (py_major, py_minor) = py_version.as_tuple();
        match if_major.cmp(&py_major) {
            Ordering::Less => true,
            Ordering::Equal => {
                if let Some(if_minor) = if_version_iter.next() {
                    // Check the if_minor number (the minor version).
                    if or_equal {
                        *if_minor <= py_minor
                    } else {
                        *if_minor < py_minor
                    }
                } else {
                    // Assume Python 3.0.
                    true
                }
            }
            Ordering::Greater => false,
        }
    } else {
        false
    }
}

/// Convert a [`Stmt::If`], retaining the `else`.
fn fix_py2_block(
    checker: &Checker,
    stmt: &Stmt,
    orelse: &[Stmt],
    block: &BlockMetadata,
) -> Option<Fix> {
    let leading_token = &block.leading_token;
    let Some(trailing_token) = &block.trailing_token else {
        // Delete the entire statement. If this is an `elif`, know it's the only child
        // of its parent, so avoid passing in the parent at all. Otherwise,
        // `delete_stmt` will erroneously include a `pass`.
        let stmt = checker.semantic().stmt();
        let parent = checker.semantic().stmt_parent();
        let edit = delete_stmt(
            stmt,
            if matches!(block.leading_token.tok, StartTok::If) {
                parent
            } else {
                None
            },
            checker.locator,
            checker.indexer,
        );
        return Some(Fix::suggested(edit));
    };

    match (&leading_token.tok, &trailing_token.tok) {
        // If we only have an `if` and an `else`, dedent the `else` block.
        (StartTok::If, EndTok::Else) => {
            let start = orelse.first()?;
            let end = orelse.last()?;
            if indentation(checker.locator, start).is_none() {
                // Inline `else` block (e.g., `else: x = 1`).
                Some(Fix::suggested(Edit::range_replacement(
                    checker
                        .locator
                        .slice(TextRange::new(start.start(), end.end()))
                        .to_string(),
                    stmt.range(),
                )))
            } else {
                indentation(checker.locator, stmt)
                    .and_then(|indentation| {
                        adjust_indentation(
                            TextRange::new(checker.locator.line_start(start.start()), end.end()),
                            indentation,
                            checker.locator,
                            checker.stylist,
                        )
                        .ok()
                    })
                    .map(|contents| {
                        Fix::suggested(Edit::replacement(
                            contents,
                            checker.locator.line_start(stmt.start()),
                            stmt.end(),
                        ))
                    })
            }
        }
        (StartTok::If, EndTok::Elif) => {
            // If we have an `if` and an `elif`, turn the `elif` into an `if`.
            let start_location = leading_token.range.start();
            let end_location = trailing_token.range.start() + TextSize::from(2);
            Some(Fix::suggested(Edit::deletion(start_location, end_location)))
        }
        (StartTok::Elif, _) => {
            // If we have an `elif`, delete up to the `else` or the end of the statement.
            let start_location = leading_token.range.start();
            let end_location = trailing_token.range.start();
            Some(Fix::suggested(Edit::deletion(start_location, end_location)))
        }
    }
}

/// Convert a [`Stmt::If`], removing the `else` block.
fn fix_py3_block(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    block: &BlockMetadata,
) -> Option<Fix> {
    match block.leading_token.tok {
        StartTok::If => {
            // If the first statement is an if, use the body of this statement, and ignore
            // the rest.
            let start = body.first()?;
            let end = body.last()?;
            if indentation(checker.locator, start).is_none() {
                // Inline `if` block (e.g., `if ...: x = 1`).
                Some(Fix::suggested(Edit::range_replacement(
                    checker
                        .locator
                        .slice(TextRange::new(start.start(), end.end()))
                        .to_string(),
                    stmt.range(),
                )))
            } else {
                indentation(checker.locator, stmt)
                    .and_then(|indentation| {
                        adjust_indentation(
                            TextRange::new(checker.locator.line_start(start.start()), end.end()),
                            indentation,
                            checker.locator,
                            checker.stylist,
                        )
                        .ok()
                    })
                    .map(|contents| {
                        Fix::suggested(Edit::replacement(
                            contents,
                            checker.locator.line_start(stmt.start()),
                            stmt.end(),
                        ))
                    })
            }
        }
        StartTok::Elif => {
            // Replace the `elif` with an `else, preserve the body of the elif, and remove
            // the rest.
            let end = body.last()?;
            let text = checker.locator.slice(TextRange::new(test.end(), end.end()));
            Some(Fix::suggested(Edit::range_replacement(
                format!("else{text}"),
                stmt.range(),
            )))
        }
    }
}

/// UP036
pub(crate) fn outdated_version_block(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = &test
    else {
        return;
    };

    if !checker
        .semantic()
        .resolve_call_path(left)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["sys", "version_info"])
        })
    {
        return;
    }

    if ops.len() == 1 && comparators.len() == 1 {
        let comparison = &comparators[0];
        let op = &ops[0];
        match comparison {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let version = extract_version(elts);
                let target = checker.settings.target_version;
                if op == &CmpOp::Lt || op == &CmpOp::LtE {
                    if compare_version(&version, target, op == &CmpOp::LtE) {
                        let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, stmt.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(block) = metadata(checker.locator, stmt, body) {
                                if let Some(fix) = fix_py2_block(checker, stmt, orelse, &block) {
                                    diagnostic.set_fix(fix);
                                }
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                } else if op == &CmpOp::Gt || op == &CmpOp::GtE {
                    if compare_version(&version, target, op == &CmpOp::GtE) {
                        let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, stmt.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(block) = metadata(checker.locator, stmt, body) {
                                if let Some(fix) = fix_py3_block(checker, stmt, test, body, &block)
                                {
                                    diagnostic.set_fix(fix);
                                }
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(number),
                ..
            }) => {
                let version_number = bigint_to_u32(number);
                if version_number == 2 && op == &CmpOp::Eq {
                    let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, stmt.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(block) = metadata(checker.locator, stmt, body) {
                            if let Some(fix) = fix_py2_block(checker, stmt, orelse, &block) {
                                diagnostic.set_fix(fix);
                            }
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                } else if version_number == 3 && op == &CmpOp::Eq {
                    let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, stmt.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(block) = metadata(checker.locator, stmt, body) {
                            if let Some(fix) = fix_py3_block(checker, stmt, test, body, &block) {
                                diagnostic.set_fix(fix);
                            }
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
            _ => (),
        }
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
