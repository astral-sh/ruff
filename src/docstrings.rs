use rustpython_ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

#[derive(Debug)]
pub enum DocstringKind {
    Module,
    Function,
    Class,
}

#[derive(Debug)]
pub struct Docstring<'a> {
    pub kind: DocstringKind,
    pub parent: Option<&'a Stmt>,
    pub expr: &'a Expr,
}

/// Extract a `Docstring` from an `Expr`.
pub fn extract<'a, 'b>(
    checker: &'a Checker,
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
                    parent: None,
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
                            DocstringKind::Class
                        } else {
                            DocstringKind::Function
                        },
                        parent: None,
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
                return;
            }
        }

        if non_empty_line_count == 1 && line_count > 1 {
            checker.add_check(Check::new(CheckKind::FitsOnOneLine, range_for(docstring)));
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
                if first_line == "\"\"\""
                    || first_line == "'''"
                    || first_line == "u\"\"\""
                    || first_line == "u'''"
                    || first_line == "r\"\"\""
                    || first_line == "r'''"
                    || first_line == "ur\"\"\""
                    || first_line == "ur'''"
                {
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

/// D403
pub fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if !matches!(docstring.kind, DocstringKind::Function) {
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
pub fn not_empty(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if string.trim().is_empty() {
            checker.add_check(Check::new(CheckKind::NonEmpty, range_for(docstring)));
        }
    }
}
