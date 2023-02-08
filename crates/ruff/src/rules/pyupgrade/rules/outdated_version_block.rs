use std::cmp::Ordering;

use log::error;
use num_bigint::{BigInt, Sign};
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located, Location, Stmt};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::{Range, RefEquality};
use crate::ast::whitespace::indentation;
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pyupgrade::fixes::adjust_indentation;
use crate::settings::types::PythonVersion;
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct OutdatedVersionBlock;
);
impl AlwaysAutofixableViolation for OutdatedVersionBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Version block is outdated for minimum Python version")
    }

    fn autofix_title(&self) -> String {
        "Remove outdated version block".to_string()
    }
}

#[derive(Debug)]
struct BlockMetadata {
    /// The first non-whitespace token in the block.
    starter: Tok,
    /// The location of the first `elif` token, if any.
    elif: Option<Location>,
    /// The location of the `else` token, if any.
    else_: Option<Location>,
}

impl BlockMetadata {
    const fn new(starter: Tok, elif: Option<Location>, else_: Option<Location>) -> Self {
        Self {
            starter,
            elif,
            else_,
        }
    }
}

fn metadata<T>(locator: &Locator, located: &Located<T>) -> Option<BlockMetadata> {
    indentation(locator, located)?;

    // Start the selection at the start-of-line. This ensures consistent indentation
    // in the token stream, in the event that the entire block is indented.
    let text = locator.slice_source_code_range(&Range::new(
        Location::new(located.location.row(), 0),
        located.end_location.unwrap(),
    ));

    let mut starter: Option<Tok> = None;
    let mut elif = None;
    let mut else_ = None;

    for (start, tok, _) in
        lexer::make_tokenizer_located(text, Location::new(located.location.row(), 0))
            .flatten()
            .filter(|(_, tok, _)| {
                !matches!(
                    tok,
                    Tok::Indent
                        | Tok::Dedent
                        | Tok::NonLogicalNewline
                        | Tok::Newline
                        | Tok::Comment(..)
                )
            })
    {
        if starter.is_none() {
            starter = Some(tok.clone());
        } else {
            if matches!(tok, Tok::Elif) && elif.is_none() {
                elif = Some(start);
            }
            if matches!(tok, Tok::Else) && else_.is_none() {
                else_ = Some(start);
            }
        }
        if starter.is_some() && elif.is_some() && else_.is_some() {
            break;
        }
    }
    Some(BlockMetadata::new(starter.unwrap(), elif, else_))
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

/// Convert a [`StmtKind::If`], retaining the `else`.
fn fix_py2_block(
    checker: &mut Checker,
    stmt: &Stmt,
    body: &[Stmt],
    orelse: &[Stmt],
    block: &BlockMetadata,
) -> Option<Fix> {
    if orelse.is_empty() {
        // Delete the entire statement. If this is an `elif`, know it's the only child
        // of its parent, so avoid passing in the parent at all. Otherwise,
        // `delete_stmt` will erroneously include a `pass`.
        let deleted: Vec<&Stmt> = checker
            .deletions
            .iter()
            .map(std::convert::Into::into)
            .collect();
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        return match delete_stmt(
            defined_by.into(),
            if block.starter == Tok::If {
                defined_in.map(std::convert::Into::into)
            } else {
                None
            },
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(fix) => {
                checker.deletions.insert(RefEquality(defined_by.into()));
                Some(fix)
            }
            Err(err) => {
                error!("Failed to remove block: {}", err);
                None
            }
        };
    }

    // If we only have an `if` and an `else`, dedent the `else` block.
    if block.starter == Tok::If && block.elif.is_none() {
        let start = orelse.first().unwrap();
        let end = orelse.last().unwrap();

        if indentation(checker.locator, start).is_none() {
            // Inline `else` block (e.g., `else: x = 1`).
            Some(Fix::replacement(
                checker
                    .locator
                    .slice_source_code_range(&Range::new(start.location, end.end_location.unwrap()))
                    .to_string(),
                stmt.location,
                stmt.end_location.unwrap(),
            ))
        } else {
            indentation(checker.locator, stmt)
                .and_then(|indentation| {
                    adjust_indentation(
                        Range::new(
                            Location::new(start.location.row(), 0),
                            end.end_location.unwrap(),
                        ),
                        indentation,
                        checker.locator,
                        checker.stylist,
                    )
                    .ok()
                })
                .map(|contents| {
                    Fix::replacement(
                        contents,
                        Location::new(stmt.location.row(), 0),
                        stmt.end_location.unwrap(),
                    )
                })
        }
    } else {
        let mut end_location = orelse.last().unwrap().location;
        if block.starter == Tok::If && block.elif.is_some() {
            // Turn the `elif` into an `if`.
            end_location = block.elif.unwrap();
            end_location.go_right();
            end_location.go_right();
        } else if block.starter == Tok::Elif {
            if let Some(elif) = block.elif {
                end_location = elif;
            } else if let Some(else_) = block.else_ {
                end_location = else_;
            } else {
                end_location = body.last().unwrap().end_location.unwrap();
            }
        }
        Some(Fix::deletion(stmt.location, end_location))
    }
}

/// Convert a [`StmtKind::If`], removing the `else` block.
fn fix_py3_block(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    block: &BlockMetadata,
) -> Option<Fix> {
    match block.starter {
        Tok::If => {
            // If the first statement is an if, use the body of this statement, and ignore
            // the rest.
            let start = body.first().unwrap();
            let end = body.last().unwrap();

            if indentation(checker.locator, start).is_none() {
                // Inline `if` block (e.g., `if ...: x = 1`).
                Some(Fix::replacement(
                    checker
                        .locator
                        .slice_source_code_range(&Range::new(
                            start.location,
                            end.end_location.unwrap(),
                        ))
                        .to_string(),
                    stmt.location,
                    stmt.end_location.unwrap(),
                ))
            } else {
                indentation(checker.locator, stmt)
                    .and_then(|indentation| {
                        adjust_indentation(
                            Range::new(
                                Location::new(start.location.row(), 0),
                                end.end_location.unwrap(),
                            ),
                            indentation,
                            checker.locator,
                            checker.stylist,
                        )
                        .ok()
                    })
                    .map(|contents| {
                        Fix::replacement(
                            contents,
                            Location::new(stmt.location.row(), 0),
                            stmt.end_location.unwrap(),
                        )
                    })
            }
        }
        Tok::Elif => {
            // Replace the `elif` with an `else, preserve the body of the elif, and remove
            // the rest.
            let end = body.last().unwrap();
            let text = checker.locator.slice_source_code_range(&Range::new(
                test.end_location.unwrap(),
                end.end_location.unwrap(),
            ));
            Some(Fix::replacement(
                format!("else{text}"),
                stmt.location,
                stmt.end_location.unwrap(),
            ))
        }
        _ => None,
    }
}

/// UP036
pub fn outdated_version_block(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
) {
    let ExprKind::Compare {
        left,
        ops,
        comparators,
    } = &test.node else {
        return;
    };

    if !checker.resolve_call_path(left).map_or(false, |call_path| {
        call_path.as_slice() == ["sys", "version_info"]
    }) {
        return;
    }

    if ops.len() == 1 && comparators.len() == 1 {
        let comparison = &comparators[0].node;
        let op = &ops[0];
        match comparison {
            ExprKind::Tuple { elts, .. } => {
                let version = extract_version(elts);
                let target = checker.settings.target_version;
                if op == &Cmpop::Lt || op == &Cmpop::LtE {
                    if compare_version(&version, target, op == &Cmpop::LtE) {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock, Range::from_located(stmt));
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(block) = metadata(checker.locator, stmt) {
                                if let Some(fix) =
                                    fix_py2_block(checker, stmt, body, orelse, &block)
                                {
                                    diagnostic.amend(fix);
                                }
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                } else if op == &Cmpop::Gt || op == &Cmpop::GtE {
                    if compare_version(&version, target, op == &Cmpop::GtE) {
                        let mut diagnostic =
                            Diagnostic::new(OutdatedVersionBlock, Range::from_located(stmt));
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(block) = metadata(checker.locator, stmt) {
                                if let Some(fix) = fix_py3_block(checker, stmt, test, body, &block)
                                {
                                    diagnostic.amend(fix);
                                }
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
            ExprKind::Constant {
                value: Constant::Int(number),
                ..
            } => {
                let version_number = bigint_to_u32(number);
                if version_number == 2 && op == &Cmpop::Eq {
                    let mut diagnostic =
                        Diagnostic::new(OutdatedVersionBlock, Range::from_located(stmt));
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(block) = metadata(checker.locator, stmt) {
                            if let Some(fix) = fix_py2_block(checker, stmt, body, orelse, &block) {
                                diagnostic.amend(fix);
                            }
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                } else if version_number == 3 && op == &Cmpop::Eq {
                    let mut diagnostic =
                        Diagnostic::new(OutdatedVersionBlock, Range::from_located(stmt));
                    if checker.patch(diagnostic.kind.rule()) {
                        if let Some(block) = metadata(checker.locator, stmt) {
                            if let Some(fix) = fix_py3_block(checker, stmt, test, body, &block) {
                                diagnostic.amend(fix);
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
