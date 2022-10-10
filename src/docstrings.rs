use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

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

/// Extract a docstring from an expression.
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
            checker.add_check(Check::new(
                CheckKind::OneLinerDocstring,
                Range::from_located(docstring.expr),
            ));
        }
    }
}

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
                CheckKind::BlankLineAfterSummary,
                Range::from_located(docstring.expr),
            ));
        }
    }
}

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
                    .slice_source_code_range(&Range::from_located(docstring.expr));
                if let Some(line) = content.lines().last() {
                    let line = line.trim();
                    if line != "\"\"\"" && line != "'''" {
                        checker.add_check(Check::new(
                            CheckKind::NewLineAfterLastParagraph,
                            Range::from_located(docstring.expr),
                        ));
                    }
                }
                return;
            }
        }
    }
}

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
                    Range::from_located(docstring.expr),
                ));
            }
        }
    }
}

pub fn not_empty(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if string.trim().is_empty() {
            checker.add_check(Check::new(
                CheckKind::EmptyDocstring,
                Range::from_located(docstring.expr),
            ));
        }
    }
}

pub fn ends_with_period(checker: &mut Checker, docstring: &Docstring) {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &docstring.expr.node
    {
        if let Some(string) = string.lines().next() {
            if !string.ends_with('.') {
                checker.add_check(Check::new(
                    CheckKind::DocstringEndsInNonPeriod,
                    Range::from_located(docstring.expr),
                ));
            }
        }
    }
}
