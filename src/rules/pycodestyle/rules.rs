use itertools::izip;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arguments, Constant, Excepthandler, Location, Stmt, StmtKind, Unaryop};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::helpers;
use crate::ast::helpers::{
    create_expr, except_range, match_leading_content, match_trailing_content, unparse_expr,
};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::settings::Settings;
use crate::source_code::{Generator, Locator, Stylist};
use crate::violations;

static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://\S+$").unwrap());

fn is_overlong(
    line: &str,
    line_length: usize,
    limit: usize,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
) -> bool {
    if line_length <= limit {
        return false;
    }

    let mut chunks = line.split_whitespace();
    let (Some(first), Some(second)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return false;
    };

    if first == "#" {
        if ignore_overlong_task_comments {
            let second = second.trim_end_matches(':');
            if task_tags.iter().any(|tag| tag == second) {
                return false;
            }
        }

        // Do not enforce the line length for commented lines that end with a URL
        // or contain only a single word.
        if chunks.last().map_or(true, |c| URL_REGEX.is_match(c)) {
            return false;
        }
    }

    true
}

/// E501
pub fn line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let line_length = line.chars().count();
    let limit = settings.line_length;
    if is_overlong(
        line,
        line_length,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    ) {
        Some(Diagnostic::new(
            violations::LineTooLong(line_length, limit),
            Range::new(
                Location::new(lineno + 1, limit),
                Location::new(lineno + 1, line_length),
            ),
        ))
    } else {
        None
    }
}

/// W505
pub fn doc_line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let Some(limit) = settings.pycodestyle.max_doc_length else {
        return None;
    };

    let line_length = line.chars().count();
    if is_overlong(
        line,
        line_length,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    ) {
        Some(Diagnostic::new(
            violations::DocLineTooLong(line_length, limit),
            Range::new(
                Location::new(lineno + 1, limit),
                Location::new(lineno + 1, line_length),
            ),
        ))
    } else {
        None
    }
}

fn compare(left: &Expr, ops: &[Cmpop], comparators: &[Expr], stylist: &Stylist) -> String {
    unparse_expr(
        &create_expr(ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        }),
        stylist,
    )
}

/// E711, E712
pub fn literal_comparisons(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
) {
    // Mapping from (bad operator index) to (replacement operator). As we iterate
    // through the list of operators, we apply "dummy" fixes for each error,
    // then replace the entire expression at the end with one "real" fix, to
    // avoid conflicts.
    let mut bad_ops: FxHashMap<usize, Cmpop> = FxHashMap::default();
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let op = ops.first().unwrap();

    // Check `left`.
    let mut comparator = left;
    let next = &comparators[0];
    if check_none_comparisons
        && matches!(
            comparator.node,
            ExprKind::Constant {
                value: Constant::None,
                kind: None
            }
        )
    {
        if matches!(op, Cmpop::Eq) {
            let diagnostic = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(diagnostic.kind.code()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::Is);
            }
            diagnostics.push(diagnostic);
        }
        if matches!(op, Cmpop::NotEq) {
            let diagnostic = Diagnostic::new(
                violations::NoneComparison(op.into()),
                Range::from_located(comparator),
            );
            if checker.patch(diagnostic.kind.code()) && !helpers::is_constant_non_singleton(next) {
                bad_ops.insert(0, Cmpop::IsNot);
            }
            diagnostics.push(diagnostic);
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                let diagnostic = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(diagnostic.kind.code())
                    && !helpers::is_constant_non_singleton(next)
                {
                    bad_ops.insert(0, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic = Diagnostic::new(
                    violations::TrueFalseComparison(value, op.into()),
                    Range::from_located(comparator),
                );
                if checker.patch(diagnostic.kind.code())
                    && !helpers::is_constant_non_singleton(next)
                {
                    bad_ops.insert(0, Cmpop::IsNot);
                }
                diagnostics.push(diagnostic);
            }
        }
    }

    // Check each comparator in order.
    for (idx, (op, next)) in izip!(ops, comparators).enumerate() {
        if check_none_comparisons
            && matches!(
                next.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None
                }
            )
        {
            if matches!(op, Cmpop::Eq) {
                let diagnostic = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(diagnostic.kind.code())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::Is);
                }
                diagnostics.push(diagnostic);
            }
            if matches!(op, Cmpop::NotEq) {
                let diagnostic = Diagnostic::new(
                    violations::NoneComparison(op.into()),
                    Range::from_located(next),
                );
                if checker.patch(diagnostic.kind.code())
                    && !helpers::is_constant_non_singleton(comparator)
                {
                    bad_ops.insert(idx, Cmpop::IsNot);
                }
                diagnostics.push(diagnostic);
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = next.node
            {
                if matches!(op, Cmpop::Eq) {
                    let diagnostic = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(diagnostic.kind.code())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::Is);
                    }
                    diagnostics.push(diagnostic);
                }
                if matches!(op, Cmpop::NotEq) {
                    let diagnostic = Diagnostic::new(
                        violations::TrueFalseComparison(value, op.into()),
                        Range::from_located(next),
                    );
                    if checker.patch(diagnostic.kind.code())
                        && !helpers::is_constant_non_singleton(comparator)
                    {
                        bad_ops.insert(idx, Cmpop::IsNot);
                    }
                    diagnostics.push(diagnostic);
                }
            }
        }

        comparator = next;
    }

    // TODO(charlie): Respect `noqa` directives. If one of the operators has a
    // `noqa`, but another doesn't, both will be removed here.
    if !bad_ops.is_empty() {
        // Replace the entire comparison expression.
        let ops = ops
            .iter()
            .enumerate()
            .map(|(idx, op)| bad_ops.get(&idx).unwrap_or(op))
            .cloned()
            .collect::<Vec<_>>();
        let content = compare(left, &ops, comparators, checker.stylist);
        for diagnostic in &mut diagnostics {
            diagnostic.amend(Fix::replacement(
                content.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }

    checker.diagnostics.extend(diagnostics);
}

/// E713, E714
pub fn not_tests(
    checker: &mut Checker,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) {
    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare {
            left,
            ops,
            comparators,
            ..
        } = &operand.node
        {
            let should_fix = ops.len() == 1;
            for op in ops.iter() {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            let mut diagnostic = Diagnostic::new(
                                violations::NotInTest,
                                Range::from_located(operand),
                            );
                            if checker.patch(diagnostic.kind.code()) && should_fix {
                                diagnostic.amend(Fix::replacement(
                                    compare(left, &[Cmpop::NotIn], comparators, checker.stylist),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            let mut diagnostic = Diagnostic::new(
                                violations::NotIsTest,
                                Range::from_located(operand),
                            );
                            if checker.patch(diagnostic.kind.code()) && should_fix {
                                diagnostic.amend(Fix::replacement(
                                    compare(left, &[Cmpop::IsNot], comparators, checker.stylist),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// E721
pub fn type_comparison(ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    for (op, right) in izip!(ops, comparators) {
        if !matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            continue;
        }
        match &right.node {
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name { id, .. } = &func.node {
                    // Ex) type(False)
                    if id == "type" {
                        if let Some(arg) = args.first() {
                            // Allow comparison for types which are not obvious.
                            if !matches!(
                                arg.node,
                                ExprKind::Name { .. }
                                    | ExprKind::Constant {
                                        value: Constant::None,
                                        kind: None
                                    }
                            ) {
                                diagnostics
                                    .push(Diagnostic::new(violations::TypeComparison, location));
                            }
                        }
                    }
                }
            }
            ExprKind::Attribute { value, .. } => {
                if let ExprKind::Name { id, .. } = &value.node {
                    // Ex) types.IntType
                    if id == "types" {
                        diagnostics.push(Diagnostic::new(violations::TypeComparison, location));
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

/// E722
pub fn do_not_use_bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &Excepthandler,
    locator: &Locator,
) -> Option<Diagnostic> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt.node, StmtKind::Raise { exc: None, .. }))
    {
        Some(Diagnostic::new(
            violations::DoNotUseBareExcept,
            except_range(handler, locator),
        ))
    } else {
        None
    }
}

fn function(name: &str, args: &Arguments, body: &Expr, stylist: &Stylist) -> String {
    let body = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Return {
            value: Some(Box::new(body.clone())),
        },
    );
    let func = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::FunctionDef {
            name: name.to_string(),
            args: Box::new(args.clone()),
            body: vec![body],
            decorator_list: vec![],
            returns: None,
            type_comment: None,
        },
    );
    let mut generator: Generator = stylist.into();
    generator.unparse_stmt(&func);
    generator.generate()
}

/// E731
pub fn do_not_assign_lambda(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            let mut diagnostic = Diagnostic::new(
                violations::DoNotAssignLambda(id.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(diagnostic.kind.code()) {
                if !match_leading_content(stmt, checker.locator)
                    && !match_trailing_content(stmt, checker.locator)
                {
                    let first_line = checker.locator.slice_source_code_range(&Range::new(
                        Location::new(stmt.location.row(), 0),
                        Location::new(stmt.location.row() + 1, 0),
                    ));
                    let indentation = &leading_space(&first_line);
                    let mut indented = String::new();
                    for (idx, line) in function(id, args, body, checker.stylist)
                        .lines()
                        .enumerate()
                    {
                        if idx == 0 {
                            indented.push_str(line);
                        } else {
                            indented.push('\n');
                            indented.push_str(indentation);
                            indented.push_str(line);
                        }
                    }
                    diagnostic.amend(Fix::replacement(
                        indented,
                        stmt.location,
                        stmt.end_location.unwrap(),
                    ));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// E741
pub fn ambiguous_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}

/// E742
pub fn ambiguous_class_name<F>(name: &str, locate: F) -> Option<Diagnostic>
where
    F: FnOnce() -> Range,
{
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousClassName(name.to_string()),
            locate(),
        ))
    } else {
        None
    }
}

/// E743
pub fn ambiguous_function_name<F>(name: &str, locate: F) -> Option<Diagnostic>
where
    F: FnOnce() -> Range,
{
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousFunctionName(name.to_string()),
            locate(),
        ))
    } else {
        None
    }
}

/// W292
pub fn no_newline_at_end_of_file(contents: &str, autofix: bool) -> Option<Diagnostic> {
    if !contents.ends_with('\n') {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = contents.lines().last() {
            // Both locations are at the end of the file (and thus the same).
            let location = Location::new(contents.lines().count(), line.len());
            let mut diagnostic = Diagnostic::new(
                violations::NoNewLineAtEndOfFile,
                Range::new(location, location),
            );
            if autofix {
                diagnostic.amend(Fix::insertion("\n".to_string(), location));
            }
            return Some(diagnostic);
        }
    }
    None
}

// See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
const VALID_ESCAPE_SEQUENCES: &[char; 23] = &[
    '\n', '\\', '\'', '"', 'a', 'b', 'f', 'n', 'r', 't', 'v', '0', '1', '2', '3', '4', '5', '6',
    '7', 'x', // Escape sequences only recognized in string literals
    'N', 'u', 'U',
];

/// Return the quotation markers used for a String token.
fn extract_quote(text: &str) -> &str {
    for quote in ["'''", "\"\"\"", "'", "\""] {
        if text.ends_with(quote) {
            return quote;
        }
    }

    panic!("Unable to find quotation mark for String token")
}

/// W605
pub fn invalid_escape_sequence(
    locator: &Locator,
    start: Location,
    end: Location,
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice_source_code_range(&Range::new(start, end));

    // Determine whether the string is single- or triple-quoted.
    let quote = extract_quote(&text);
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[(quote_pos + quote.len())..(text.len() - quote.len())];

    if !prefix.contains('r') {
        for (row_offset, line) in body.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for col_offset in 0..chars.len() {
                if chars[col_offset] != '\\' {
                    continue;
                }

                // If the previous character was also a backslash, skip.
                if col_offset > 0 && chars[col_offset - 1] == '\\' {
                    continue;
                }

                // If we're at the end of the line, skip.
                if col_offset == chars.len() - 1 {
                    continue;
                }

                // If the next character is a valid escape sequence, skip.
                let next_char = chars[col_offset + 1];
                if VALID_ESCAPE_SEQUENCES.contains(&next_char) {
                    continue;
                }

                // Compute the location of the escape sequence by offsetting the location of the
                // string token by the characters we've seen thus far.
                let col = if row_offset == 0 {
                    start.column() + prefix.len() + quote.len() + col_offset
                } else {
                    col_offset
                };
                let location = Location::new(start.row() + row_offset, col);
                let end_location = Location::new(location.row(), location.column() + 2);
                let mut diagnostic = Diagnostic::new(
                    violations::InvalidEscapeSequence(next_char),
                    Range::new(location, end_location),
                );
                if autofix {
                    diagnostic.amend(Fix::insertion(r"\".to_string(), location));
                }
                diagnostics.push(diagnostic);
            }
        }
    }

    diagnostics
}
