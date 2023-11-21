use std::hash::BuildHasherDefault;

use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::AstNode;
use ruff_python_ast::{self as ast, Arguments, Decorator, Expr, ExprContext};
use ruff_python_codegen::Generator;
use ruff_python_trivia::CommentRanges;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::super::types;
use super::helpers::{is_pytest_parametrize, split_names};

/// ## What it does
/// Checks for the type of parameter names passed to `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// The `argnames` argument of `pytest.mark.parametrize` takes a string or
/// a sequence of strings. For a single parameter, it's preferable to use a
/// string. For multiple parameters, it's preferable to use the style
/// configured via the [`flake8-pytest-style.parametrize-names-type`] setting.
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
    expected: types::ParametrizeNameType,
}

impl Violation for PytestParametrizeNamesWrongType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeNamesWrongType { expected } = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn fix_title(&self) -> Option<String> {
        let PytestParametrizeNamesWrongType { expected } = self;
        Some(format!("Use a `{expected}` for parameter names"))
    }
}

/// ## What it does
/// Checks for the type of parameter values passed to `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// The `argvalues` argument of `pytest.mark.parametrize` takes an iterator of
/// parameter values, which can be provided as lists or tuples.
///
/// To aid in readability, it's recommended to use a consistent style for the
/// list of values rows, and, in the case of multiple parameters, for each row
/// of values.
///
/// The style for the list of values rows can be configured via the
/// the [`flake8-pytest-style.parametrize-values-type`] setting, while the
/// style for each row of values can be configured via the
/// the [`flake8-pytest-style.parametrize-values-row-type`] setting.
///
/// For example, [`flake8-pytest-style.parametrize-values-type`] will lead to
/// the following expectations:
///
/// - `tuple`: `@pytest.mark.parametrize("value", ("a", "b", "c"))`
/// - `list`: `@pytest.mark.parametrize("value", ["a", "b", "c"])`
///
/// Similarly, [`flake8-pytest-style.parametrize-values-row-type`] will lead to
/// the following expectations:
///
/// - `tuple`: `@pytest.mark.parametrize(("key", "value"), [("a", "b"), ("c", "d")])`
/// - `list`: `@pytest.mark.parametrize(("key", "value"), [["a", "b"], ["c", "d"]])`
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
/// - `flake8-pytest-style.parametrize-values-type`
/// - `flake8-pytest-style.parametrize-values-row-type`
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
#[violation]
pub struct PytestParametrizeValuesWrongType {
    values: types::ParametrizeValuesType,
    row: types::ParametrizeValuesRowType,
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
    index: usize,
}

impl Violation for PytestDuplicateParametrizeTestCases {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestDuplicateParametrizeTestCases { index } = self;
        format!("Duplicate of test case at index {index} in `@pytest_mark.parametrize`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate test case".to_string())
    }
}

fn elts_to_csv(elts: &[Expr], generator: Generator) -> Option<String> {
    if !elts.iter().all(Expr::is_string_literal_expr) {
        return None;
    }

    let node = Expr::from(ast::StringLiteral {
        value: elts.iter().fold(String::new(), |mut acc, elt| {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = elt {
                if !acc.is_empty() {
                    acc.push(',');
                }
                acc.push_str(value.as_str());
            }
            acc
        }),
        ..ast::StringLiteral::default()
    });
    Some(generator.expr(&node))
}

/// Returns the range of the `name` argument of `@pytest.mark.parametrize`.
///
/// This accounts for parenthesized expressions. For example, the following code
/// will return the range marked with `^`:
/// ```python
/// @pytest.mark.parametrize(("x"), [(1, 2)])
/// #                        ^^^^^
/// def test(a, b):
///     ...
/// ```
///
/// This method assumes that the first argument is a string.
fn get_parametrize_name_range(
    decorator: &Decorator,
    expr: &Expr,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Option<TextRange> {
    decorator.expression.as_call_expr().and_then(|call| {
        parenthesized_range(
            expr.into(),
            call.arguments.as_any_node_ref(),
            comment_ranges,
            source,
        )
    })
}

/// PT006
fn check_names(checker: &mut Checker, decorator: &Decorator, expr: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value: string, .. }) => {
            let names = split_names(string);
            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let name_range = get_parametrize_name_range(
                            decorator,
                            expr,
                            checker.indexer().comment_ranges(),
                            checker.locator().contents(),
                        )
                        .unwrap_or(expr.range());
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        let node = Expr::Tuple(ast::ExprTuple {
                            elts: names
                                .iter()
                                .map(|name| {
                                    Expr::from(ast::StringLiteral {
                                        value: (*name).to_string(),
                                        ..ast::StringLiteral::default()
                                    })
                                })
                                .collect(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            format!("({})", checker.generator().expr(&node)),
                            name_range,
                        )));
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::List => {
                        let name_range = get_parametrize_name_range(
                            decorator,
                            expr,
                            checker.indexer().comment_ranges(),
                            checker.locator().contents(),
                        )
                        .unwrap_or(expr.range());
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            name_range,
                        );
                        let node = Expr::List(ast::ExprList {
                            elts: names
                                .iter()
                                .map(|name| {
                                    Expr::from(ast::StringLiteral {
                                        value: (*name).to_string(),
                                        ..ast::StringLiteral::default()
                                    })
                                })
                                .collect(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            checker.generator().expr(&node),
                            name_range,
                        )));
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
                        let node = Expr::List(ast::ExprList {
                            elts: elts.clone(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            checker.generator().expr(&node),
                            expr.range(),
                        )));
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if let Some(content) = elts_to_csv(elts, checker.generator()) {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                content,
                                expr.range(),
                            )));
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
                        let node = Expr::Tuple(ast::ExprTuple {
                            elts: elts.clone(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            format!("({})", checker.generator().expr(&node)),
                            expr.range(),
                        )));
                        checker.diagnostics.push(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if let Some(content) = elts_to_csv(elts, checker.generator()) {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                content,
                                expr.range(),
                            )));
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

    let is_multi_named =
        if let Expr::StringLiteral(ast::ExprStringLiteral { value: string, .. }) = &names {
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

/// Given an element in a list, return the comma that follows it:
/// ```python
/// @pytest.mark.parametrize(
///     "x",
///     [.., (elt), ..],
///              ^^^^^
///              Tokenize this range to locate the comma.
/// )
/// ```
fn trailing_comma(element: &Expr, source: &str) -> Option<TextSize> {
    SimpleTokenizer::starts_at(element.end(), source)
        .find(|token| token.kind == SimpleTokenKind::Comma)
        .map(|token| token.start())
}

/// PT014
fn check_duplicates(checker: &mut Checker, values: &Expr) {
    let (Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) =
        values
    else {
        return;
    };

    let mut seen: FxHashMap<ComparableExpr, usize> =
        FxHashMap::with_capacity_and_hasher(elts.len(), BuildHasherDefault::default());
    let mut prev = None;
    for (index, element) in elts.iter().enumerate() {
        let expr = ComparableExpr::from(element);
        seen.entry(expr)
            .and_modify(|index| {
                let mut diagnostic = Diagnostic::new(
                    PytestDuplicateParametrizeTestCases { index: *index },
                    element.range(),
                );
                if let Some(prev) = prev {
                    let values_end = values.range().end() - TextSize::new(1);
                    let previous_end =
                        trailing_comma(prev, checker.locator().contents()).unwrap_or(values_end);
                    let element_end =
                        trailing_comma(element, checker.locator().contents()).unwrap_or(values_end);
                    let deletion_range = TextRange::new(previous_end, element_end);
                    if !checker
                        .indexer()
                        .comment_ranges()
                        .intersects(deletion_range)
                    {
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(deletion_range)));
                    }
                }
                checker.diagnostics.push(diagnostic);
            })
            .or_insert(index);
        prev = Some(element);
    }
}

fn handle_single_name(checker: &mut Checker, expr: &Expr, value: &Expr) {
    let mut diagnostic = Diagnostic::new(
        PytestParametrizeNamesWrongType {
            expected: types::ParametrizeNameType::Csv,
        },
        expr.range(),
    );

    let node = value.clone();
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        checker.generator().expr(&node),
        expr.range(),
    )));
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
                    if let [names, ..] = args.as_slice() {
                        check_names(checker, decorator, names);
                    }
                }
                if checker.enabled(Rule::PytestParametrizeValuesWrongType) {
                    if let [names, values, ..] = args.as_slice() {
                        check_values(checker, names, values);
                    }
                }
                if checker.enabled(Rule::PytestDuplicateParametrizeTestCases) {
                    if let [_, values, ..] = args.as_slice() {
                        check_duplicates(checker, values);
                    }
                }
            }
        }
    }
}
