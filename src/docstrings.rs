use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

#[derive(Debug)]
pub enum DocstringKind<'a> {
    Module,
    Function(&'a Stmt),
    Class(&'a Stmt),
}

#[derive(Debug)]
pub struct Docstring<'a> {
    pub kind: DocstringKind<'a>,
    pub expr: &'a Expr,
}

/// Extract a `Docstring` from an `Expr`.
pub fn extract<'a, 'b>(
    checker: &'a Checker<'b>,
    stmt: &'b Stmt,
    expr: &'b Expr,
) -> Option<Docstring<'b>> {
    let defined_in = checker
        .binding_context()
        .defined_in
        .map(|index| checker.parents[index]);
    match defined_in {
        None => {
            if checker.initial {
                return Some(Docstring {
                    kind: DocstringKind::Module,
                    expr,
                });
            }
        }
        Some(parent) => {
            if let StmtKind::FunctionDef { body, .. }
            | StmtKind::AsyncFunctionDef { body, .. }
            | StmtKind::ClassDef { body, .. } = &parent.node
            {
                if body.first().map(|node| node == stmt).unwrap_or_default() {
                    return Some(Docstring {
                        kind: if matches!(&parent.node, StmtKind::ClassDef { .. }) {
                            DocstringKind::Class(parent)
                        } else {
                            DocstringKind::Function(parent)
                        },
                        expr,
                    });
                }
            }
        }
    }

    None
}

/// Extract the source code range for a `Docstring`.
fn range_for(docstring: &Docstring) -> Range {
    // RustPython currently omits the first quotation mark in a string, so offset the location.
    Range {
        location: Location::new(
            docstring.expr.location.row(),
            docstring.expr.location.column() - 1,
        ),
        end_location: docstring.expr.end_location,
    }
}

/// D200
pub fn one_liner(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        let mut line_count = 0;
        let mut non_empty_line_count = 0;
        for line in string.lines() {
            line_count += 1;
            if !line.trim().is_empty() {
                non_empty_line_count += 1;
            }
            if non_empty_line_count > 1 {
                break;
            }
        }

        if non_empty_line_count == 1 && line_count > 1 {
            checker.add_check(Check::new(CheckKind::FitsOnOneLine, range_for(docstring)));
        }
    }
}

static COMMENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*#").unwrap());

static INNER_FUNCTION_OR_CLASS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s+(?:(?:class|def|async def)\s|@)").unwrap());

/// D201, D202
pub fn blank_before_after_function(checker: &mut Checker, docstring: &Docstring) {
    if let DocstringKind::Function(parent) = &docstring.kind {
        if let ExprKind::Constant {
            value: Constant::Str(_),
            ..
        } = &docstring.expr.node
        {
            let (before, _, after) = checker
                .locator
                .partition_source_code_at(&Range::from_located(parent), &range_for(docstring));

            if checker.settings.enabled.contains(&CheckCode::D201) {
                let blank_lines_before = before
                    .lines()
                    .rev()
                    .skip(1)
                    .take_while(|line| line.trim().is_empty())
                    .count();
                if blank_lines_before != 0 {
                    checker.add_check(Check::new(
                        CheckKind::NoBlankLineBeforeFunction(blank_lines_before),
                        range_for(docstring),
                    ));
                }
            }

            if checker.settings.enabled.contains(&CheckCode::D202) {
                let blank_lines_after = after
                    .lines()
                    .skip(1)
                    .take_while(|line| line.trim().is_empty())
                    .count();
                let all_blank_after = after
                    .lines()
                    .skip(1)
                    .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                // Report a D202 violation if the docstring is followed by a blank line
                // and the blank line is not itself followed by an inner function or
                // class.
                if !all_blank_after
                    && blank_lines_after != 0
                    && !(blank_lines_after == 1 && INNER_FUNCTION_OR_CLASS_REGEX.is_match(after))
                {
                    checker.add_check(Check::new(
                        CheckKind::NoBlankLineAfterFunction(blank_lines_after),
                        range_for(docstring),
                    ));
                }
            }
        }
    }
}

/// D203, D204, D211
pub fn blank_before_after_class(checker: &mut Checker, docstring: &Docstring) {
    if let DocstringKind::Class(parent) = &docstring.kind {
        if let ExprKind::Constant {
            value: Constant::Str(_),
            ..
        } = &docstring.expr.node
        {
            let (before, _, after) = checker
                .locator
                .partition_source_code_at(&Range::from_located(parent), &range_for(docstring));

            if checker.settings.enabled.contains(&CheckCode::D203)
                || checker.settings.enabled.contains(&CheckCode::D211)
            {
                let blank_lines_before = before
                    .lines()
                    .rev()
                    .skip(1)
                    .take_while(|line| line.trim().is_empty())
                    .count();
                if blank_lines_before != 0 && checker.settings.enabled.contains(&CheckCode::D211) {
                    checker.add_check(Check::new(
                        CheckKind::NoBlankLineBeforeClass(blank_lines_before),
                        range_for(docstring),
                    ));
                }
                if blank_lines_before != 1 && checker.settings.enabled.contains(&CheckCode::D203) {
                    checker.add_check(Check::new(
                        CheckKind::OneBlankLineBeforeClass(blank_lines_before),
                        range_for(docstring),
                    ));
                }
            }

            if checker.settings.enabled.contains(&CheckCode::D204) {
                let blank_lines_after = after
                    .lines()
                    .skip(1)
                    .take_while(|line| line.trim().is_empty())
                    .count();
                let all_blank_after = after
                    .lines()
                    .skip(1)
                    .all(|line| line.trim().is_empty() || COMMENT_REGEX.is_match(line));
                if !all_blank_after && blank_lines_after != 1 {
                    checker.add_check(Check::new(
                        CheckKind::OneBlankLineAfterClass(blank_lines_after),
                        range_for(docstring),
                    ));
                }
            }
        }
    }
}

/// D205
pub fn blank_after_summary(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        let mut lines_count = 1;
        let mut blanks_count = 0;
        for line in string.trim().lines().skip(1) {
            lines_count += 1;
            if line.trim().is_empty() {
                blanks_count += 1;
            } else {
                break;
            }
        }
        if lines_count > 1 && blanks_count != 1 {
            checker.add_check(Check::new(
                CheckKind::NoBlankLineAfterSummary,
                range_for(docstring),
            ));
        }
    }
}

/// D209
pub fn newline_after_last_paragraph(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        let mut line_count = 0;
        for line in string.lines() {
            if !line.trim().is_empty() {
                line_count += 1;
            }
            if line_count > 1 {
                let content = checker
                    .locator
                    .slice_source_code_range(&range_for(docstring));
                if let Some(line) = content.lines().last() {
                    let line = line.trim();
                    if line != "\"\"\"" && line != "'''" {
                        checker.add_check(Check::new(
                            CheckKind::NewLineAfterLastParagraph,
                            range_for(docstring),
                        ));
                    }
                }
                return;
            }
        }
    }
}

/// D210
pub fn no_surrounding_whitespace(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        let mut lines = string.lines();
        if let Some(line) = lines.next() {
            if line.trim().is_empty() {
                return;
            }
            if line.starts_with(' ') || (matches!(lines.next(), None) && line.ends_with(' ')) {
                checker.add_check(Check::new(
                    CheckKind::NoSurroundingWhitespace,
                    range_for(docstring),
                ));
            }
        }
    }
}

/// D212, D213
pub fn multi_line_summary_start(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if string.lines().nth(1).is_some() {
            let content = checker
                .locator
                .slice_source_code_range(&range_for(docstring));
            if let Some(first_line) = content.lines().next() {
                let first_line = first_line.trim();
                if first_line == "\"\"\"" || first_line == "'''" {
                    if checker.settings.enabled.contains(&CheckCode::D212) {
                        checker.add_check(Check::new(
                            CheckKind::MultiLineSummaryFirstLine,
                            range_for(docstring),
                        ));
                    }
                } else if checker.settings.enabled.contains(&CheckCode::D213) {
                    checker.add_check(Check::new(
                        CheckKind::MultiLineSummarySecondLine,
                        range_for(docstring),
                    ));
                }
            }
        }
    }
}

/// D300
pub fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        let content = checker
            .locator
            .slice_source_code_range(&range_for(docstring));
        if string.contains("\"\"\"") {
            if !content.starts_with("'''") {
                checker.add_check(Check::new(
                    CheckKind::UsesTripleQuotes,
                    range_for(docstring),
                ));
            }
        } else if !content.starts_with("\"\"\"") {
            checker.add_check(Check::new(
                CheckKind::UsesTripleQuotes,
                range_for(docstring),
            ));
        }
    }
}

/// D400
pub fn ends_with_period(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if let Some(string) = string.lines().next() {
            if !string.ends_with('.') {
                checker.add_check(Check::new(CheckKind::EndsInPeriod, range_for(docstring)));
            }
        }
    }
}

/// D402
pub fn no_signature(checker: &mut Checker, docstring: &Docstring) {
    if let DocstringKind::Function(parent) = docstring.kind {
        if let StmtKind::FunctionDef { name, .. } = &parent.node {
            if let ExprKind::Constant {
                value: Constant::Str(string),
                ..
            } = &docstring.expr.node
            {
                if let Some(first_line) = string.lines().next() {
                    if first_line.contains(&format!("{name}(")) {
                        checker.add_check(Check::new(CheckKind::NoSignature, range_for(docstring)));
                    }
                }
            }
        }
    }
}

/// D403
pub fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if !matches!(docstring.kind, DocstringKind::Function(_)) {
        return;
    }

    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if let Some(first_word) = string.split(' ').next() {
            if first_word == first_word.to_uppercase() {
                return;
            }
            for char in first_word.chars() {
                if !char.is_ascii_alphabetic() && char != '\'' {
                    return;
                }
            }
            if let Some(first_char) = first_word.chars().next() {
                if !first_char.is_uppercase() {
                    checker.add_check(Check::new(
                        CheckKind::FirstLineCapitalized,
                        range_for(docstring),
                    ));
                }
            }
        }
    }
}

/// D415
pub fn ends_with_punctuation(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if let Some(string) = string.lines().next() {
            if !(string.ends_with('.') || string.ends_with('!') || string.ends_with('?')) {
                checker.add_check(Check::new(
                    CheckKind::EndsInPunctuation,
                    range_for(docstring),
                ));
            }
        }
    }
}

/// D419
pub fn not_empty(checker: &mut Checker, docstring: &Docstring) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if string.trim().is_empty() {
            if checker.settings.enabled.contains(&CheckCode::D419) {
                checker.add_check(Check::new(CheckKind::NonEmpty, range_for(docstring)));
            }
            return false;
        }
    }
    true
}
