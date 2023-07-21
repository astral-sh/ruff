use std::cmp::Ordering;

use num_bigint::{BigInt, Sign};
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{self, CmpOp, Constant, ElifElseClause, Expr, Ranged, StmtIf};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::stmt_if::{if_elif_branches, BranchKind, IfElifBranch};
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

/// For fixing, we have 4 cases:
/// * Just an if: delete as statement (insert pass in parent if required)
/// * If with an elif: delete, turn elif into if
/// * If with an else: delete, dedent else
/// * Just an elif: delete, `elif False` can always be removed
fn fix_py2_block(checker: &Checker, stmt_if: &StmtIf, branch: &IfElifBranch) -> Option<Fix> {
    match branch.kind {
        BranchKind::If => match stmt_if.elif_else_clauses.first() {
            // If we have a lone `if`, delete as statement (insert pass in parent if required)
            None => {
                let stmt = checker.semantic().stmt();
                let parent = checker.semantic().stmt_parent();
                let edit = delete_stmt(stmt, parent, checker.locator, checker.indexer);
                Some(Fix::suggested(edit))
            }
            // If we have an `if` and an `elif`, turn the `elif` into an `if`
            Some(ElifElseClause {
                test: Some(_),
                range,
                ..
            }) => {
                debug_assert!(
                    &checker.locator.contents()[TextRange::at(range.start(), "elif".text_len())]
                        == "elif"
                );
                let end_location = range.start() + ("elif".text_len() - "if".text_len());
                Some(Fix::suggested(Edit::deletion(
                    stmt_if.start(),
                    end_location,
                )))
            }
            // If we only have an `if` and an `else`, dedent the `else` block
            Some(ElifElseClause {
                body, test: None, ..
            }) => {
                let start = body.first()?;
                let end = body.last()?;
                if indentation(checker.locator, start).is_none() {
                    // Inline `else` block (e.g., `else: x = 1`).
                    Some(Fix::suggested(Edit::range_replacement(
                        checker
                            .locator
                            .slice(TextRange::new(start.start(), end.end()))
                            .to_string(),
                        stmt_if.range(),
                    )))
                } else {
                    indentation(checker.locator, stmt_if)
                        .and_then(|indentation| {
                            adjust_indentation(
                                TextRange::new(
                                    checker.locator.line_start(start.start()),
                                    end.end(),
                                ),
                                indentation,
                                checker.locator,
                                checker.stylist,
                            )
                            .ok()
                        })
                        .map(|contents| {
                            Fix::suggested(Edit::replacement(
                                contents,
                                checker.locator.line_start(stmt_if.start()),
                                stmt_if.end(),
                            ))
                        })
                }
            }
        },
        BranchKind::Elif => {
            // The range of the `ElifElseClause` ends in the line of the last statement. To avoid
            // inserting an empty line between the end of `if` branch and the beginning `elif` or
            // `else` branch after the deleted branch we find the next branch after the current, if
            // any, and delete to its start.
            // ```python
            //                         if cond:
            //                             x = 1
            //                         elif sys.version < (3.0):
            //    delete from here ... ^   x = 2
            //                         else:
            // ... to here (exclusive) ^    x = 3
            // ```
            let next_start = stmt_if
                .elif_else_clauses
                .iter()
                .map(Ranged::start)
                .find(|start| *start > branch.range.start());
            Some(Fix::suggested(Edit::deletion(
                branch.range.start(),
                next_start.unwrap_or(branch.range.end()),
            )))
        }
    }
}

/// Convert a [`Stmt::If`], removing the `else` block.
fn fix_py3_block(checker: &mut Checker, stmt_if: &StmtIf, branch: &IfElifBranch) -> Option<Fix> {
    match branch.kind {
        BranchKind::If => {
            // If the first statement is an `if`, use the body of this statement, and ignore
            // the rest.
            let start = branch.body.first()?;
            let end = branch.body.last()?;
            if indentation(checker.locator, start).is_none() {
                // Inline `if` block (e.g., `if ...: x = 1`).
                Some(Fix::suggested(Edit::range_replacement(
                    checker
                        .locator
                        .slice(TextRange::new(start.start(), end.end()))
                        .to_string(),
                    stmt_if.range,
                )))
            } else {
                indentation(checker.locator, &stmt_if)
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
                            checker.locator.line_start(stmt_if.start()),
                            stmt_if.end(),
                        ))
                    })
            }
        }
        BranchKind::Elif => {
            // Replace the `elif` with an `else`, preserve the body of the elif, and remove
            // the rest.
            let end = branch.body.last()?;
            let text = checker
                .locator
                .slice(TextRange::new(branch.test.end(), end.end()));
            Some(Fix::suggested(Edit::range_replacement(
                format!("else{text}"),
                TextRange::new(branch.range.start(), stmt_if.end()),
            )))
        }
    }
}

/// UP036
pub(crate) fn outdated_version_block(checker: &mut Checker, stmt_if: &StmtIf) {
    for branch in if_elif_branches(stmt_if) {
        let Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        }) = &branch.test
        else {
            continue;
        };

        let ([op], [comparison]) = (ops.as_slice(), comparators.as_slice()) else {
            continue;
        };

        if !checker
            .semantic()
            .resolve_call_path(left)
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["sys", "version_info"])
            })
        {
            continue;
        }

        match comparison {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let version = extract_version(elts);
                let target = checker.settings.target_version;
                if op == &CmpOp::Lt || op == &CmpOp::LtE {
                    if compare_version(&version, target, op == &CmpOp::LtE) {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock, branch.test.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(fix) = fix_py2_block(checker, stmt_if, &branch) {
                                diagnostic.set_fix(fix);
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                } else if op == &CmpOp::Gt || op == &CmpOp::GtE {
                    if compare_version(&version, target, op == &CmpOp::GtE) {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock, branch.test.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(fix) = fix_py3_block(checker, stmt_if, &branch) {
                                diagnostic.set_fix(fix);
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
                    let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, branch.test.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(fix) = fix_py2_block(checker, stmt_if, &branch) {
                            diagnostic.set_fix(fix);
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                } else if version_number == 3 && op == &CmpOp::Eq {
                    let mut diagnostic = Diagnostic::new(OutdatedVersionBlock, branch.test.range());
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(fix) = fix_py3_block(checker, stmt_if, &branch) {
                            diagnostic.set_fix(fix);
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
