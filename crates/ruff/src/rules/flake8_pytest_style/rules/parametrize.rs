use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Decorator, Expr, ExprContext, Ranged};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Generator, Locator};

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::super::types;
use super::helpers::{is_pytest_parametrize, split_names};

#[violation]
pub struct PytestParametrizeNamesWrongType {
    pub expected: types::ParametrizeNameType,
}

impl Violation for PytestParametrizeNamesWrongType {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeNamesWrongType { expected } = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn autofix_title(&self) -> Option<String> {
        let PytestParametrizeNamesWrongType { expected } = self;
        Some(format!("Use a `{expected}` for parameter names"))
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

fn elts_to_csv(elts: &[Expr], generator: Generator) -> Option<String> {
    let all_literals = elts.iter().all(|e| {
        matches!(
            e,
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            })
        )
    });

    if !all_literals {
        return None;
    }

    let node = Expr::Constant(ast::ExprConstant {
        value: Constant::Str(elts.iter().fold(String::new(), |mut acc, elt| {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(ref s),
                ..
            }) = elt
            {
                if !acc.is_empty() {
                    acc.push(',');
                }
                acc.push_str(s);
            }
            acc
        })),
        kind: None,
        range: TextRange::default(),
    });
    Some(generator.expr(&node))
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
fn get_parametrize_name_range(decorator: &Decorator, expr: &Expr, locator: &Locator) -> TextRange {
    let mut locations = Vec::new();
    let mut implicit_concat = None;

    // The parenthesis are not part of the AST, so we need to tokenize the
    // decorator to find them.
    for (tok, range) in lexer::lex_starts_at(
        locator.slice(decorator.range()),
        Mode::Module,
        decorator.start(),
    )
    .flatten()
    {
        match tok {
            Tok::Lpar => locations.push(range.start()),
            Tok::Rpar => {
                if let Some(start) = locations.pop() {
                    implicit_concat = Some(TextRange::new(start, range.end()));
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
        expr.range()
    }
}

/// PT006
fn check_names(checker: &mut Checker, decorator: &Decorator, expr: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(string),
            ..
        }) => {
            let names = split_names(string);
            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let name_range =
                            get_parametrize_name_range(decorator, expr, checker.locator);
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let node = Expr::Tuple(ast::ExprTuple {
                                elts: names
                                    .iter()
                                    .map(|name| {
                                        Expr::Constant(ast::ExprConstant {
                                            value: Constant::Str((*name).to_string()),
                                            kind: None,
                                            range: TextRange::default(),
                                        })
                                    })
                                    .collect(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            });
                            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                format!("({})", checker.generator().expr(&node)),
                                name_range,
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::List => {
                        let name_range =
                            get_parametrize_name_range(decorator, expr, checker.locator);
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let node = Expr::List(ast::ExprList {
                                elts: names
                                    .iter()
                                    .map(|name| {
                                        Expr::Constant(ast::ExprConstant {
                                            value: Constant::Str((*name).to_string()),
                                            kind: None,
                                            range: TextRange::default(),
                                        })
                                    })
                                    .collect(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            });
                            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                checker.generator().expr(&node),
                                name_range,
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {}
                }
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
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
                            expr.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let node = Expr::List(ast::ExprList {
                                elts: elts.clone(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            });
                            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                checker.generator().expr(&node),
                                expr.range(),
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker.generator()) {
                                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                    content,
                                    expr.range(),
                                )));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            };
        }
        Expr::List(ast::ExprList { elts, .. }) => {
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
                            expr.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            let node = Expr::Tuple(ast::ExprTuple {
                                elts: elts.clone(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            });
                            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                format!("({})", checker.generator().expr(&node)),
                                expr.range(),
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(content) = elts_to_csv(elts, checker.generator()) {
                                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                    content,
                                    expr.range(),
                                )));
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

    let is_multi_named = if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(string),
        ..
    }) = &names
    {
        split_names(string).len() > 1
    } else {
        true
    };

    match values {
        Expr::List(ast::ExprList { elts, .. }) => {
            if values_type != types::ParametrizeValuesType::List {
                checker.diagnostics.push(Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    values.range(),
                ));
            }
            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            if values_type != types::ParametrizeValuesType::Tuple {
                checker.diagnostics.push(Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    values.range(),
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
        expr.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        let node = value.clone();
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            checker.generator().expr(&node),
            expr.range(),
        )));
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
        match elt {
            Expr::Tuple(_) => {
                if values_row_type != types::ParametrizeValuesRowType::Tuple {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        elt.range(),
                    ));
                }
            }
            Expr::List(_) => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        elt.range(),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn parametrize(checker: &mut Checker, decorators: &[Decorator]) {
    for decorator in decorators {
        if is_pytest_parametrize(decorator, checker.semantic()) {
            if let Expr::Call(ast::ExprCall { args, .. }) = &decorator.expression {
                if checker.enabled(Rule::PytestParametrizeNamesWrongType) {
                    if let Some(names) = args.get(0) {
                        check_names(checker, decorator, names);
                    }
                }
                if checker.enabled(Rule::PytestParametrizeValuesWrongType) {
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
