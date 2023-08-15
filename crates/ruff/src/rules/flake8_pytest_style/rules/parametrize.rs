use ruff_python_ast::{
    self as ast, Arguments, Constant, Decorator, Expr, ExprContext, PySourceType, Ranged,
};
use ruff_python_parser::{lexer, AsMode, Tok};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_codegen::Generator;
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::super::types;
use super::helpers::{is_pytest_parametrize, split_names};

/// ## What it does
/// Checks for the type of parameter names passed to `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// The `argnames` argument of `pytest.mark.parametrize` takes a string or
/// a sequence of strings. For a single parameter, it's preferable to use a
/// string, and for multiple parameters, it's preferable to use the style
/// configured via the `flake8-pytest-style.parametrize-names-type` setting.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// # single parameter, always expecting string
/// @pytest.mark.parametrize(("param",), [1, 2, 3])
/// def test_foo(param):
///     ...
///
///
/// # multiple parameters, expecting tuple
/// @pytest.mark.parametrize(["param1", "param2"], [(1, 2), (3, 4)])
/// def test_bar(param1, param2):
///     ...
///
///
/// # multiple parameters, expecting tuple
/// @pytest.mark.parametrize("param1,param2", [(1, 2), (3, 4)])
/// def test_baz(param1, param2):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize("param", [1, 2, 3])
/// def test_foo(param):
///     ...
///
///
/// @pytest.mark.parametrize(("param1", "param2"), [(1, 2), (3, 4)])
/// def test_bar(param1, param2):
///     ...
/// ```
///
/// ## Options
/// - `flake8-pytest-style.parametrize-names-type`
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
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

/// ## What it does
/// Checks for the type of parameter values passed to `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// The `argvalues` argument of `pytest.mark.parametrize` takes an iterator of
/// parameter values. For a single parameter, it's preferable to use a list,
/// and for multiple parameters, it's preferable to use a list of rows with
/// the type configured via the `flake8-pytest-style.parametrize-values-row-type`
/// setting.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// # expected list, got tuple
/// @pytest.mark.parametrize("param", (1, 2))
/// def test_foo(param):
///     ...
///
///
/// # expected top-level list, got tuple
/// @pytest.mark.parametrize(
///     ("param1", "param2"),
///     (
///         (1, 2),
///         (3, 4),
///     ),
/// )
/// def test_bar(param1, param2):
///     ...
///
///
/// # expected individual rows to be tuples, got lists
/// @pytest.mark.parametrize(
///     ("param1", "param2"),
///     [
///         [1, 2],
///         [3, 4],
///     ],
/// )
/// def test_baz(param1, param2):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize("param", [1, 2, 3])
/// def test_foo(param):
///     ...
///
///
/// @pytest.mark.parametrize(("param1", "param2"), [(1, 2), (3, 4)])
/// def test_bar(param1, param2):
///     ...
/// ```
///
/// ## Options
/// - `flake8-pytest-style.parametrize-values-row-type`
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
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

/// ## What it does
/// Checks for duplicate test cases in `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// Duplicate test cases are redundant and should be removed.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize(
///     ("param1", "param2"),
///     [
///         (1, 2),
///         (1, 2),
///     ],
/// )
/// def test_foo(param1, param2):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize(
///     ("param1", "param2"),
///     [
///         (1, 2),
///     ],
/// )
/// def test_foo(param1, param2):
///     ...
/// ```
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
#[violation]
pub struct PytestDuplicateParametrizeTestCases {
    pub indices: Vec<usize>,
}

impl Violation for PytestDuplicateParametrizeTestCases {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestDuplicateParametrizeTestCases { indices } = self;
        format!("Found duplicate test cases {indices:?} in `@pytest.mark.parametrize`")
    }
}

fn elts_to_csv(elts: &[Expr], generator: Generator) -> Option<String> {
    let all_literals = elts.iter().all(|expr| {
        matches!(
            expr,
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
        value: elts
            .iter()
            .fold(String::new(), |mut acc, elt| {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(ast::StringConstant { value, .. }),
                    ..
                }) = elt
                {
                    if !acc.is_empty() {
                        acc.push(',');
                    }
                    acc.push_str(value.as_str());
                }
                acc
            })
            .into(),

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
fn get_parametrize_name_range(
    decorator: &Decorator,
    expr: &Expr,
    locator: &Locator,
    source_type: PySourceType,
) -> TextRange {
    let mut locations = Vec::new();
    let mut implicit_concat = None;

    // The parenthesis are not part of the AST, so we need to tokenize the
    // decorator to find them.
    for (tok, range) in lexer::lex_starts_at(
        locator.slice(decorator.range()),
        source_type.as_mode(),
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
                        let name_range = get_parametrize_name_range(
                            decorator,
                            expr,
                            checker.locator(),
                            checker.source_type,
                        );
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
                                            value: (*name).to_string().into(),
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
                        let name_range = get_parametrize_name_range(
                            decorator,
                            expr,
                            checker.locator(),
                            checker.source_type,
                        );
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
                                            value: (*name).to_string().into(),
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

fn find_duplicates(elts: &Vec<Expr>) -> Vec<usize> {
    let mut duplicates: Vec<usize> = Vec::with_capacity(elts.len());
    let mut seen_values: FxHashSet<ComparableExpr> = FxHashSet::default();
    for (idx, elt) in elts.iter().enumerate() {
        let comparable_value: ComparableExpr = elt.into();
        if !seen_values.insert(comparable_value) {
            duplicates.push(idx);
        }
    }
    duplicates
}

/// PT014
fn check_duplicates(checker: &mut Checker, values: &Expr) {
    match values {
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            let indices = find_duplicates(elts);
            if !indices.is_empty() {
                checker.diagnostics.push(Diagnostic::new(
                    PytestDuplicateParametrizeTestCases { indices },
                    values.range(),
                ));
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
            if let Expr::Call(ast::ExprCall {
                arguments: Arguments { args, .. },
                ..
            }) = &decorator.expression
            {
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
                if checker.enabled(Rule::PytestDuplicateParametrizeTestCases) {
                    if let [_, values, ..] = &args[..] {
                        check_duplicates(checker, values);
                    }
                }
            }
        }
    }
}
