use std::cmp::Ordering;
use std::ptr;

use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::stmt_if::{BranchKind, IfElifBranch, if_elif_branches};
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{self as ast, CmpOp, ElifElseClause, Expr, Int, PythonVersion, Stmt, StmtIf};
use ruff_python_semantic::SemanticModel;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::{adjust_indentation, delete_stmt};
use crate::preview::is_outdated_version_check_enabled;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `sys.version_info` comparisons that are outdated for the minimum
/// supported Python version.
///
/// ## Why is this bad?
/// In Python, code can be conditionally executed based on the active
/// Python version by comparing against the `sys.version_info` tuple.
///
/// If a comparison always evaluates to the same value for every supported Python
/// version, the code it guards is either dead or unconditional, and should be
/// simplified. For example, if a code block is only executed for Python versions
/// older than the minimum supported version, it should be removed.
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
/// By default, this rule only applies to comparisons that make up the entire test
/// of an `if` or `elif` branch. In [preview], it flags every outdated
/// `sys.version_info` comparison, wherever it appears:
///
/// ```python
/// import sys
///
/// PY2 = sys.version_info < (3, 0)
///
/// if force_legacy or sys.version_info < (3, 0):
///     print("py2")
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## Fix availability
/// A fix is only offered when the entire test of an `if` or `elif` branch is made up of
/// `sys.version_info` comparisons, since only then is it clear which code is
/// unreachable. This includes chained comparisons such as
/// `if (3, 8) <= sys.version_info < (3, 10)`, as long as every link is a version check.
/// Elsewhere, such as in an assignment or alongside an unrelated condition, replacing
/// the comparison with `True` or `False` would be sound, but it would defeat the
/// purpose of flagging obsolete code that can be migrated.
///
/// ## Fix safety
/// This rule's fix is marked as unsafe because it will remove all code,
/// comments, and annotations within unreachable version blocks.
///
/// ## References
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.240", category = Category::Suspicious)]
pub(crate) struct OutdatedVersionBlock {
    reason: Reason,
    /// Whether the comparison makes up the entire test of an `if` or `elif` branch,
    /// in which case the branch as a whole (not just the comparison) is outdated.
    is_block: bool,
}

impl Violation for OutdatedVersionBlock {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.reason {
            Reason::AlwaysFalse | Reason::AlwaysTrue => {
                if self.is_block {
                    "Version block is outdated for minimum Python version".to_string()
                } else {
                    "Version check is outdated for minimum Python version".to_string()
                }
            }
            Reason::Invalid => "Version specifier is invalid".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.reason {
            Reason::AlwaysFalse | Reason::AlwaysTrue if self.is_block => {
                Some("Remove outdated version block".to_string())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Reason {
    AlwaysTrue,
    AlwaysFalse,
    Invalid,
}

/// One adjacent pair of a (possibly chained) comparison, normalized to the
/// `sys.version_info <op> <version>` form.
struct Link {
    /// Why the check is outdated, or `None` if this pair is not a decidable version check.
    reason: Option<Reason>,
    /// Where to report: the version operand for an invalid specifier, the whole pair otherwise.
    range: TextRange,
    /// Whether `sys.version_info` was written on the left-hand side.
    version_info_on_left: bool,
}

/// Whether an entire branch test is known to evaluate the same way on every supported version.
#[derive(Debug, Copy, Clone)]
enum BranchVerdict {
    AlwaysTrue,
    AlwaysFalse,
}

impl From<BranchVerdict> for Reason {
    fn from(verdict: BranchVerdict) -> Self {
        match verdict {
            BranchVerdict::AlwaysTrue => Reason::AlwaysTrue,
            BranchVerdict::AlwaysFalse => Reason::AlwaysFalse,
        }
    }
}

/// UP036
pub(crate) fn outdated_version_block(checker: &Checker, expr: &Expr, compare: &ast::ExprCompare) {
    let branch = gated_branch(checker.semantic(), expr);
    let is_chained = compare.ops.len() > 1;

    // `a < b < c` is equivalent to `a < b and b < c`, so judge each adjacent pair on its own.
    let links: Vec<Link> = compare
        .ops
        .iter()
        .zip(&compare.comparators)
        .enumerate()
        .map(|(index, (op, right))| {
            let left = if index == 0 {
                &*compare.left
            } else {
                &compare.comparators[index - 1]
            };
            // For a lone comparison, prefer the range of the comparison itself, so that any
            // parentheses around the left operand are covered.
            let link_range = if is_chained {
                TextRange::new(left.start(), right.end())
            } else {
                compare.range()
            };

            // Normalize to `sys.version_info <op> <version>`, mirroring the operator when the
            // check is written the other way around (e.g., `(3, 0) > sys.version_info`).
            let (op, comparison, version_info_on_left) =
                if is_valid_version_info(checker.semantic(), left) {
                    (*op, right, true)
                } else if is_valid_version_info(checker.semantic(), right)
                    && let Some(mirrored) = mirror(*op)
                {
                    (mirrored, left, false)
                } else {
                    return Link {
                        reason: None,
                        range: link_range,
                        version_info_on_left: false,
                    };
                };

            let reason = version_check_reason(op, comparison, checker.target_version());
            Link {
                reason,
                range: if reason == Some(Reason::Invalid) {
                    comparison.range()
                } else {
                    link_range
                },
                version_info_on_left,
            }
        })
        .collect();

    // Outside of preview, the rule is limited to a single canonical
    // `sys.version_info <op> <version>` comparison gating an entire branch.
    let stable_shape = branch.is_some()
        && !is_chained
        && links.first().is_some_and(|link| link.version_info_on_left);
    if !stable_shape && !is_outdated_version_check_enabled(checker.settings()) {
        return;
    }

    // When the comparison is the entire test of a branch and every link is a decidable version
    // check, the branch as a whole is either dead or unconditional, so it can be removed. Report
    // it once, against the whole test, rather than once per outdated link.
    if let Some((stmt_if, branch)) = &branch
        && let Some(verdict) = branch_verdict(&links)
    {
        let mut diagnostic = checker.report_diagnostic(
            OutdatedVersionBlock {
                reason: verdict.into(),
                is_block: true,
            },
            compare.range(),
        );
        let fix = match verdict {
            BranchVerdict::AlwaysFalse => fix_always_false_branch(checker, stmt_if, branch),
            BranchVerdict::AlwaysTrue => fix_always_true_branch(checker, stmt_if, branch),
        };
        if let Some(fix) = fix {
            diagnostic.set_fix(fix);
        }
        return;
    }

    // Otherwise the branch (if any) survives, so only the individual outdated links are at fault.
    for link in &links {
        let Some(reason) = link.reason else {
            continue;
        };
        checker.report_diagnostic(
            OutdatedVersionBlock {
                reason,
                is_block: !is_chained && branch.is_some(),
            },
            link.range,
        );
    }
}

/// Determine whether every link of a comparison gating a branch resolves the same way.
///
/// Returns `None` unless *all* links are decidable version checks. A branch guarded by anything
/// else may still be reachable, and removing it would drop a condition that matters at runtime.
fn branch_verdict(links: &[Link]) -> Option<BranchVerdict> {
    if links.is_empty() {
        return None;
    }
    // The links of a chain are joined by `and`, so one always-false link is enough to make the
    // whole test always false, while an always-true test needs every link to be always true.
    let mut verdict = BranchVerdict::AlwaysTrue;
    for link in links {
        match link.reason? {
            Reason::Invalid => return None,
            Reason::AlwaysFalse => verdict = BranchVerdict::AlwaysFalse,
            Reason::AlwaysTrue => {}
        }
    }
    Some(verdict)
}

/// If `expr` is the entire test of an `if` or `elif` branch, return that branch along with the
/// `if` statement that owns it.
fn gated_branch<'a>(
    semantic: &SemanticModel<'a>,
    expr: &Expr,
) -> Option<(&'a StmtIf, IfElifBranch<'a>)> {
    let Stmt::If(stmt_if) = semantic.current_statement() else {
        return None;
    };
    let branch = if_elif_branches(stmt_if).find(|branch| ptr::eq(branch.test, expr))?;
    Some((stmt_if, branch))
}

/// Return the operator that yields the same result once the operands are swapped, if any.
///
/// For example, `(3, 0) > sys.version_info` is equivalent to `sys.version_info < (3, 0)`.
fn mirror(op: CmpOp) -> Option<CmpOp> {
    match op {
        // Equality and identity are symmetric.
        CmpOp::Eq => Some(CmpOp::Eq),
        CmpOp::NotEq => Some(CmpOp::NotEq),
        CmpOp::Is => Some(CmpOp::Is),
        CmpOp::IsNot => Some(CmpOp::IsNot),
        CmpOp::Lt => Some(CmpOp::Gt),
        CmpOp::LtE => Some(CmpOp::GtE),
        CmpOp::Gt => Some(CmpOp::Lt),
        CmpOp::GtE => Some(CmpOp::LtE),
        // Containment is not: `a in b` says nothing about `b in a`.
        CmpOp::In | CmpOp::NotIn => None,
    }
}

/// Determine whether `sys.version_info <op> <comparison>` always evaluates to the same value for
/// every Python version supported by `target`, or is not a well-formed version check at all.
fn version_check_reason(op: CmpOp, comparison: &Expr, target: PythonVersion) -> Option<Reason> {
    match comparison {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            if !matches!(op, CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE) {
                return None;
            }
            let version = extract_version(elts)?;
            match version_always_less_than(
                &version,
                target,
                // `x <= y` and `x > y` are cases where `x == y` will not stop the comparison
                // from always evaluating to true or false respectively
                op.is_lt_e() || op.is_gt(),
            ) {
                Ok(false) => None,
                Ok(true) => Some(if op.is_lt() || op.is_lt_e() {
                    Reason::AlwaysFalse
                } else {
                    Reason::AlwaysTrue
                }),
                Err(_) => Some(Reason::Invalid),
            }
        }
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            ..
        }) => match (int.as_u8(), op) {
            (Some(2), CmpOp::Eq) => Some(Reason::AlwaysFalse),
            (Some(3), CmpOp::Eq) => Some(Reason::AlwaysTrue),
            (Some(2), CmpOp::NotEq) => Some(Reason::AlwaysTrue),
            (Some(3), CmpOp::NotEq) => Some(Reason::AlwaysFalse),
            (Some(2), CmpOp::Lt) => Some(Reason::AlwaysFalse),
            (Some(3), CmpOp::Lt) => Some(Reason::AlwaysFalse),
            (Some(2), CmpOp::LtE) => Some(Reason::AlwaysFalse),
            (Some(3), CmpOp::LtE) => Some(Reason::AlwaysTrue),
            (Some(2), CmpOp::Gt) => Some(Reason::AlwaysTrue),
            (Some(3), CmpOp::Gt) => Some(Reason::AlwaysFalse),
            (Some(2), CmpOp::GtE) => Some(Reason::AlwaysTrue),
            (Some(3), CmpOp::GtE) => Some(Reason::AlwaysTrue),
            (None, _) => Some(Reason::Invalid),
            _ => None,
        },
        _ => None,
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

            let if_micro = match check_version_iter.next() {
                None => None,
                Some(micro) => match micro.as_u8() {
                    Some(micro) => Some(micro),
                    None => anyhow::bail!("invalid micro version: {micro}"),
                },
            };

            Ok(if or_equal {
                // Ex) `sys.version_info <= 3.8`. If Python 3.8 is the minimum supported version,
                // the condition won't always evaluate to `false`, so we want to return `false`.
                if_minor < py_minor
            } else {
                if let Some(if_micro) = if_micro {
                    // Ex) `sys.version_info < 3.8.3`
                    if_minor < py_minor || if_minor == py_minor && if_micro == 0
                } else {
                    // Ex) `sys.version_info < 3.8`. If Python 3.8 is the minimum supported version,
                    // the condition _will_ always evaluate to `false`, so we want to return `true`.
                    if_minor <= py_minor
                }
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
                debug_assert_eq!(
                    checker
                        .locator()
                        .slice(TextRange::at(range.start(), "elif".text_len())),
                    "elif"
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
                if indentation(checker.source(), start).is_none() {
                    // Inline `else` block (e.g., `else: x = 1`).
                    Some(Fix::unsafe_edit(Edit::range_replacement(
                        checker
                            .locator()
                            .slice(TextRange::new(start.start(), end.end()))
                            .to_string(),
                        stmt_if.range(),
                    )))
                } else {
                    indentation(checker.source(), stmt_if)
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
    checker: &Checker,
    stmt_if: &StmtIf,
    branch: &IfElifBranch,
) -> Option<Fix> {
    match branch.kind {
        BranchKind::If => {
            // If the first statement is an `if`, use the body of this statement, and ignore
            // the rest.
            let start = branch.body.first()?;
            let end = branch.body.last()?;
            if indentation(checker.source(), start).is_none() {
                // Inline `if` block (e.g., `if ...: x = 1`).
                Some(Fix::unsafe_edit(Edit::range_replacement(
                    checker
                        .locator()
                        .slice(TextRange::new(start.start(), end.end()))
                        .to_string(),
                    stmt_if.range,
                )))
            } else {
                indentation(checker.source(), &stmt_if)
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

/// Returns `true` if the expression is related to `sys.version_info`.
///
/// This includes:
/// - Direct access: `sys.version_info`
/// - Subscript access: `sys.version_info[:2]`, `sys.version_info[0]`
/// - Major version attribute: `sys.version_info.major`
fn is_valid_version_info(semantic: &SemanticModel, left: &Expr) -> bool {
    semantic
        .resolve_qualified_name(map_subscript(left))
        .is_some_and(|name| matches!(name.segments(), ["sys", "version_info"]))
        || semantic
            .resolve_qualified_name(left)
            .is_some_and(|name| matches!(name.segments(), ["sys", "version_info", "major"]))
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::*;

    #[test_case(PythonVersion::PY37, & [2], true, true; "compare-2.0")]
    #[test_case(PythonVersion::PY37, & [2, 0], true, true; "compare-2.0-whole")]
    #[test_case(PythonVersion::PY37, & [3], true, true; "compare-3.0")]
    #[test_case(PythonVersion::PY37, & [3, 0], true, true; "compare-3.0-whole")]
    #[test_case(PythonVersion::PY37, & [3, 1], true, true; "compare-3.1")]
    #[test_case(PythonVersion::PY37, & [3, 5], true, true; "compare-3.5")]
    #[test_case(PythonVersion::PY37, & [3, 7], true, false; "compare-3.7")]
    #[test_case(PythonVersion::PY37, & [3, 7], false, true; "compare-3.7-not-equal")]
    #[test_case(PythonVersion::PY37, & [3, 8], false, false; "compare-3.8")]
    #[test_case(PythonVersion::PY310, & [3, 9], true, true; "compare-3.9")]
    #[test_case(PythonVersion::PY310, & [3, 11], true, false; "compare-3.11")]
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
