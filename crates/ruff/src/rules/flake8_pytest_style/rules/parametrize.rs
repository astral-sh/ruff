use rustpython_parser::ast::{Constant, Expr, ExprContext, ExprKind};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::super::types;
use super::helpers::{is_pytest_parametrize, split_names};

#[violation]
pub struct PytestParametrizeNamesWrongType {
    pub expected: types::ParametrizeNameType,
}

impl AlwaysAutofixableViolation for PytestParametrizeNamesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeNamesWrongType { expected } = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn autofix_title(&self) -> String {
        let PytestParametrizeNamesWrongType { expected } = self;
        format!("Use a `{expected}` for parameter names")
    }
}

#[violation]
pub struct PytestParametrizeValuesWrongType {
    pub values: types::ParametrizeValuesType,
    pub row: types::ParametrizeValuesRowType,
}

impl Violation for PytestParametrizeValuesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeValuesWrongType { values, row } = self;
        format!("Wrong values type in `@pytest.mark.parametrize` expected `{values}` of `{row}`")
    }
}

fn elts_to_csv(elts: &[Expr], checker: &Checker) -> Option<String> {
    let all_literals = elts.iter().all(|e| {
        matches!(
            e.node,
            ExprKind::Constant {
                value: Constant::Str(_),
                ..
            }
        )
    });

    if !all_literals {
        return None;
    }

    Some(unparse_expr(
        &create_expr(ExprKind::Constant {
            value: Constant::Str(elts.iter().fold(String::new(), |mut acc, elt| {
                if let ExprKind::Constant {
                    value: Constant::Str(ref s),
                    ..
                } = elt.node
                {
                    if !acc.is_empty() {
                        acc.push(',');
                    }
                    acc.push_str(s);
                }
                acc
            })),
            kind: None,
        }),
        checker.stylist,
    ))
}

/// Returns the range of the `name` argument of `@pytest.mark.parametrize`.
///
/// This accounts for implicit string concatenation with parenthesis.
/// For example, the following code will return the range marked with `^`:
/// ```python
/// @pytest.mark.parametrize(("a, " "b"), [(1, 2)])
/// #                        ^^^^^^^^^^^
/// #                        implicit string concatenation with parenthesis
/// def test(a, b):
///     ...
/// ```
///
/// This method assumes that the first argument is a string.
fn get_parametrize_name_range(checker: &Checker, decorator: &Expr, expr: &Expr) -> Range {
    let mut locations = Vec::new();
    let mut implicit_concat = None;

    // The parenthesis are not part of the AST, so we need to tokenize the
    // decorator to find them.
    for (start, tok, end) in lexer::lex_located(
        checker.locator.slice(decorator),
        Mode::Module,
        decorator.location,
    )
    .flatten()
    {
        match tok {
            Tok::Lpar => locations.push(start),
            Tok::Rpar => {
                if let Some(start) = locations.pop() {
                    implicit_concat = Some(Range::new(start, end));
                }
            }
            // Stop after the first argument.
            Tok::Comma => break,
            _ => (),
        }
    }

    if let Some(range) = implicit_concat {
        range
    } else {
        Range::from(expr)
    }
}

/// PT006
fn check_names(checker: &mut Checker, decorator: &Expr, expr: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => {
            let names = split_names(string);
            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let name_range = get_parametrize_name_range(checker, decorator, expr);
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Edit::replacement(
                                format!(
                                    "({})",
                                    unparse_expr(
                                        &create_expr(ExprKind::Tuple {
                                            elts: names
                                                .iter()
                                                .map(|&name| {
                                                    create_expr(ExprKind::Constant {
                                                        value: Constant::Str(name.to_string()),
                                                        kind: None,
                                                    })
                                                })
                                                .collect(),
                                            ctx: ExprContext::Load,
                                        }),
                                        checker.stylist,
                                    )
                                ),
                                name_range.location,
                                name_range.end_location,
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::List => {
                        let name_range = get_parametrize_name_range(checker, decorator, expr);
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Edit::replacement(
                                unparse_expr(
                                    &create_expr(ExprKind::List {
                                        elts: names
                                            .iter()
                                            .map(|&name| {
                                                create_expr(ExprKind::Constant {
                                                    value: Constant::Str(name.to_string()),
                                                    kind: None,
                                                })
                                            })
                                            .collect(),
                                        ctx: ExprContext::Load,
                                    }),
                                    checker.stylist,
                                ),
                                name_range.location,
                                name_range.end_location,
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {}
                }
            }
        }
        ExprKind::Tuple { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(checker, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::Tuple => {}
                    types::ParametrizeNameType::List => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Edit::replacement(
                                unparse_expr(
                                    &create_expr(ExprKind::List {
                                        elts: elts.clone(),
                                        ctx: ExprContext::Load,
                                    }),
                                    checker.stylist,
                                ),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                diagnostic.set_fix(Edit::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            };
        }
        ExprKind::List { elts, .. } => {
            if elts.len() == 1 {
                if let Some(first) = elts.first() {
                    handle_single_name(checker, expr, first);
                }
            } else {
                match names_type {
                    types::ParametrizeNameType::List => {}
                    types::ParametrizeNameType::Tuple => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Edit::replacement(
                                format!(
                                    "({})",
                                    unparse_expr(
                                        &create_expr(ExprKind::Tuple {
                                            elts: elts.clone(),
                                            ctx: ExprContext::Load,
                                        }),
                                        checker.stylist,
                                    )
                                ),
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            Range::from(expr),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker) {
                                diagnostic.set_fix(Edit::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            };
        }
        _ => {}
    }
}

/// PT007
fn check_values(checker: &mut Checker, names: &Expr, values: &Expr) {
    let values_type = checker.settings.flake8_pytest_style.parametrize_values_type;

    let values_row_type = checker
        .settings
        .flake8_pytest_style
        .parametrize_values_row_type;

    let is_multi_named = if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &names.node
    {
        split_names(string).len() > 1
    } else {
        true
    };

    match &values.node {
        ExprKind::List { elts, .. } => {
            if values_type != types::ParametrizeValuesType::List {
                checker.diagnostics.push(Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    Range::from(values),
                ));
            }
            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        ExprKind::Tuple { elts, .. } => {
            if values_type != types::ParametrizeValuesType::Tuple {
                checker.diagnostics.push(Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    Range::from(values),
                ));
            }
            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        _ => {}
    }
}

fn handle_single_name(checker: &mut Checker, expr: &Expr, value: &Expr) {
    let mut diagnostic = Diagnostic::new(
        PytestParametrizeNamesWrongType {
            expected: types::ParametrizeNameType::Csv,
        },
        Range::from(expr),
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            unparse_expr(&create_expr(value.node.clone()), checker.stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn handle_value_rows(
    checker: &mut Checker,
    elts: &[Expr],
    values_type: types::ParametrizeValuesType,
    values_row_type: types::ParametrizeValuesRowType,
) {
    for elt in elts {
        match &elt.node {
            ExprKind::Tuple { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::Tuple {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        Range::from(elt),
                    ));
                }
            }
            ExprKind::List { .. } => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        Range::from(elt),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub fn parametrize(checker: &mut Checker, decorators: &[Expr]) {
    for decorator in decorators {
        if is_pytest_parametrize(decorator, checker) {
            if let ExprKind::Call { args, .. } = &decorator.node {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::PytestParametrizeNamesWrongType)
                {
                    if let Some(names) = args.get(0) {
                        check_names(checker, decorator, names);
                    }
                }
                if checker
                    .settings
                    .rules
                    .enabled(Rule::PytestParametrizeValuesWrongType)
                {
                    if let Some(names) = args.get(0) {
                        if let Some(values) = args.get(1) {
                            check_values(checker, names, values);
                        }
                    }
                }
            }
        }
    }
}
