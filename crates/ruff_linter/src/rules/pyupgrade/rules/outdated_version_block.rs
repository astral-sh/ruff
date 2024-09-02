use std::cmp::Ordering;

use anyhow::Result;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::stmt_if::{if_elif_branches, BranchKind, IfElifBranch};
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{self as ast, CmpOp, ElifElseClause, Expr, Int, StmtIf};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::{adjust_indentation, delete_stmt};
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
pub struct OutdatedVersionBlock {
    reason: Reason,
}

impl Violation for OutdatedVersionBlock {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let OutdatedVersionBlock { reason } = self;
        match reason {
            Reason::AlwaysFalse | Reason::AlwaysTrue => {
                format!("Version block is outdated for minimum Python version")
            }
            Reason::Invalid => format!("Version specifier is invalid"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let OutdatedVersionBlock { reason } = self;
        match reason {
            Reason::AlwaysFalse | Reason::AlwaysTrue => {
                Some("Remove outdated version block".to_string())
            }
            Reason::Invalid => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Reason {
    AlwaysTrue,
    AlwaysFalse,
    Invalid,
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

        let ([op], [comparison]) = (&**ops, &**comparators) else {
            continue;
        };

        // Detect `sys.version_info`, along with slices (like `sys.version_info[:2]`).
        if !checker
            .semantic()
            .resolve_qualified_name(map_subscript(left))
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["sys", "version_info"])
            })
        {
            continue;
        }

        match comparison {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => match op {
                CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE => {
                    let Some(version) = extract_version(elts) else {
                        return;
                    };
                    let target = checker.settings.target_version;
                    match version_always_less_than(
                        &version,
                        target,
                        // `x <= y` and `x > y` are cases where `x == y` will not stop the comparison
                        // from always evaluating to true or false respectively
                        op.is_lt_e() || op.is_gt(),
                    ) {
                        Ok(false) => {}
                        Ok(true) => {
                            let mut diagnostic = Diagnostic::new(
                                OutdatedVersionBlock {
                                    reason: if op.is_lt() || op.is_lt_e() {
                                        Reason::AlwaysFalse
                                    } else {
                                        Reason::AlwaysTrue
                                    },
                                },
                                branch.test.range(),
                            );
                            if let Some(fix) = if op.is_lt() || op.is_lt_e() {
                                fix_always_false_branch(checker, stmt_if, &branch)
                            } else {
                                fix_always_true_branch(checker, stmt_if, &branch)
                            } {
                                diagnostic.set_fix(fix);
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                        Err(_) => {
                            checker.diagnostics.push(Diagnostic::new(
                                OutdatedVersionBlock {
                                    reason: Reason::Invalid,
                                },
                                comparison.range(),
                            ));
                        }
                    }
                }
                _ => {}
            },
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(int),
                ..
            }) => {
                let reason = match (int.as_u8(), op) {
                    (Some(2), CmpOp::Eq) => Reason::AlwaysFalse,
                    (Some(3), CmpOp::Eq) => Reason::AlwaysTrue,
                    (Some(2), CmpOp::NotEq) => Reason::AlwaysTrue,
                    (Some(3), CmpOp::NotEq) => Reason::AlwaysFalse,
                    (Some(2), CmpOp::Lt) => Reason::AlwaysFalse,
                    (Some(3), CmpOp::Lt) => Reason::AlwaysFalse,
                    (Some(2), CmpOp::LtE) => Reason::AlwaysFalse,
                    (Some(3), CmpOp::LtE) => Reason::AlwaysTrue,
                    (Some(2), CmpOp::Gt) => Reason::AlwaysTrue,
                    (Some(3), CmpOp::Gt) => Reason::AlwaysFalse,
                    (Some(2), CmpOp::GtE) => Reason::AlwaysTrue,
                    (Some(3), CmpOp::GtE) => Reason::AlwaysTrue,
                    (None, _) => Reason::Invalid,
                    _ => return,
                };
                match reason {
                    Reason::AlwaysTrue => {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock { reason }, branch.test.range());
                        if let Some(fix) = fix_always_true_branch(checker, stmt_if, &branch) {
                            diagnostic.set_fix(fix);
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    Reason::AlwaysFalse => {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock { reason }, branch.test.range());
                        if let Some(fix) = fix_always_false_branch(checker, stmt_if, &branch) {
                            diagnostic.set_fix(fix);
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    Reason::Invalid => {
                        checker.diagnostics.push(Diagnostic::new(
                            OutdatedVersionBlock {
                                reason: Reason::Invalid,
                            },
                            comparison.range(),
                        ));
                    }
                }
            }
            _ => (),
        }
    }
}

/// Returns true if the `check_version` is always less than the [`PythonVersion`].
fn version_always_less_than(
    check_version: &[Int],
    py_version: PythonVersion,
    or_equal: bool,
) -> Result<bool> {
    let mut check_version_iter = check_version.iter();

    let Some(if_major) = check_version_iter.next() else {
        return Ok(false);
    };
    let Some(if_major) = if_major.as_u8() else {
        return Err(anyhow::anyhow!("invalid major version: {if_major}"));
    };

    let (py_major, py_minor) = py_version.as_tuple();

    match if_major.cmp(&py_major) {
        Ordering::Less => Ok(true),
        Ordering::Greater => Ok(false),
        Ordering::Equal => {
            let Some(if_minor) = check_version_iter.next() else {
                return Ok(true);
            };
            let Some(if_minor) = if_minor.as_u8() else {
                return Err(anyhow::anyhow!("invalid minor version: {if_minor}"));
            };

            Ok(if or_equal {
                // Ex) `sys.version_info <= 3.8`. If Python 3.8 is the minimum supported version,
                // the condition won't always evaluate to `false`, so we want to return `false`.
                if_minor < py_minor
            } else {
                // Ex) `sys.version_info < 3.8`. If Python 3.8 is the minimum supported version,
                // the condition _will_ always evaluate to `false`, so we want to return `true`.
                if_minor <= py_minor
            })
        }
    }
}

/// Fix a branch that is known to always evaluate to `false`.
///
/// For example, when running with a minimum supported version of Python 3.8, the following branch
/// would be considered redundant:
/// ```python
/// if sys.version_info < (3, 7): ...
/// ```
///
/// In this case, the fix would involve removing the branch; however, there are multiple cases to
/// consider. For example, if the `if` has an `else`, then the `if` should be removed, and the
/// `else` should be inlined at the top level.
fn fix_always_false_branch(
    checker: &Checker,
    stmt_if: &StmtIf,
    branch: &IfElifBranch,
) -> Option<Fix> {
    match branch.kind {
        BranchKind::If => match stmt_if.elif_else_clauses.first() {
            // If we have a lone `if`, delete as statement (insert pass in parent if required)
            None => {
                let stmt = checker.semantic().current_statement();
                let parent = checker.semantic().current_statement_parent();
                let edit = delete_stmt(stmt, parent, checker.locator(), checker.indexer());
                Some(Fix::unsafe_edit(edit))
            }
            // If we have an `if` and an `elif`, turn the `elif` into an `if`
            Some(ElifElseClause {
                test: Some(_),
                range,
                ..
            }) => {
                debug_assert!(
                    checker
                        .locator()
                        .slice(TextRange::at(range.start(), "elif".text_len()))
                        == "elif"
                );
                let end_location = range.start() + ("elif".text_len() - "if".text_len());
                Some(Fix::unsafe_edit(Edit::deletion(
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
                if indentation(checker.locator(), start).is_none() {
                    // Inline `else` block (e.g., `else: x = 1`).
                    Some(Fix::unsafe_edit(Edit::range_replacement(
                        checker
                            .locator()
                            .slice(TextRange::new(start.start(), end.end()))
                            .to_string(),
                        stmt_if.range(),
                    )))
                } else {
                    indentation(checker.locator(), stmt_if)
                        .and_then(|indentation| {
                            adjust_indentation(
                                TextRange::new(
                                    checker.locator().line_start(start.start()),
                                    end.end(),
                                ),
                                indentation,
                                checker.locator(),
                                checker.indexer(),
                                checker.stylist(),
                            )
                            .ok()
                        })
                        .map(|contents| {
                            Fix::unsafe_edit(Edit::replacement(
                                contents,
                                checker.locator().line_start(stmt_if.start()),
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
                .find(|start| *start > branch.start());
            Some(Fix::unsafe_edit(Edit::deletion(
                branch.start(),
                next_start.unwrap_or(branch.end()),
            )))
        }
    }
}

/// Fix a branch that is known to always evaluate to `true`.
///
/// For example, when running with a minimum supported version of Python 3.8, the following branch
/// would be considered redundant, as it's known to always evaluate to `true`:
/// ```python
/// if sys.version_info >= (3, 8): ...
/// ```
fn fix_always_true_branch(
    checker: &mut Checker,
    stmt_if: &StmtIf,
    branch: &IfElifBranch,
) -> Option<Fix> {
    match branch.kind {
        BranchKind::If => {
            // If the first statement is an `if`, use the body of this statement, and ignore
            // the rest.
            let start = branch.body.first()?;
            let end = branch.body.last()?;
            if indentation(checker.locator(), start).is_none() {
                // Inline `if` block (e.g., `if ...: x = 1`).
                Some(Fix::unsafe_edit(Edit::range_replacement(
                    checker
                        .locator()
                        .slice(TextRange::new(start.start(), end.end()))
                        .to_string(),
                    stmt_if.range,
                )))
            } else {
                indentation(checker.locator(), &stmt_if)
                    .and_then(|indentation| {
                        adjust_indentation(
                            TextRange::new(checker.locator().line_start(start.start()), end.end()),
                            indentation,
                            checker.locator(),
                            checker.indexer(),
                            checker.stylist(),
                        )
                        .ok()
                    })
                    .map(|contents| {
                        Fix::unsafe_edit(Edit::replacement(
                            contents,
                            checker.locator().line_start(stmt_if.start()),
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
                .locator()
                .slice(TextRange::new(branch.test.end(), end.end()));
            Some(Fix::unsafe_edit(Edit::range_replacement(
                format!("else{text}"),
                TextRange::new(branch.start(), stmt_if.end()),
            )))
        }
    }
}

/// Return the version tuple as a sequence of [`Int`] values.
fn extract_version(elts: &[Expr]) -> Option<Vec<Int>> {
    let mut version: Vec<Int> = vec![];
    for elt in elts {
        let Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            ..
        }) = &elt
        else {
            return None;
        };
        version.push(int.clone());
    }
    Some(version)
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::*;

    #[test_case(PythonVersion::Py37, & [2], true, true; "compare-2.0")]
    #[test_case(PythonVersion::Py37, & [2, 0], true, true; "compare-2.0-whole")]
    #[test_case(PythonVersion::Py37, & [3], true, true; "compare-3.0")]
    #[test_case(PythonVersion::Py37, & [3, 0], true, true; "compare-3.0-whole")]
    #[test_case(PythonVersion::Py37, & [3, 1], true, true; "compare-3.1")]
    #[test_case(PythonVersion::Py37, & [3, 5], true, true; "compare-3.5")]
    #[test_case(PythonVersion::Py37, & [3, 7], true, false; "compare-3.7")]
    #[test_case(PythonVersion::Py37, & [3, 7], false, true; "compare-3.7-not-equal")]
    #[test_case(PythonVersion::Py37, & [3, 8], false, false; "compare-3.8")]
    #[test_case(PythonVersion::Py310, & [3, 9], true, true; "compare-3.9")]
    #[test_case(PythonVersion::Py310, & [3, 11], true, false; "compare-3.11")]
    fn test_compare_version(
        version: PythonVersion,
        target_versions: &[u8],
        or_equal: bool,
        expected: bool,
    ) -> Result<()> {
        let target_versions: Vec<_> = target_versions.iter().map(|int| Int::from(*int)).collect();
        let actual = version_always_less_than(&target_versions, version, or_equal)?;
        assert_eq!(actual, expected);
        Ok(())
    }
}
