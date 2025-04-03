use rustc_hash::{FxBuildHasher, FxHashMap};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr, ExprCall, ExprContext, StringLiteralFlags};
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
/// configured via the [`lint.flake8-pytest-style.parametrize-names-type`] setting.
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// # single parameter, always expecting string
/// @pytest.mark.parametrize(("param",), [1, 2, 3])
/// def test_foo(param): ...
///
///
/// # multiple parameters, expecting tuple
/// @pytest.mark.parametrize(["param1", "param2"], [(1, 2), (3, 4)])
/// def test_bar(param1, param2): ...
///
///
/// # multiple parameters, expecting tuple
/// @pytest.mark.parametrize("param1,param2", [(1, 2), (3, 4)])
/// def test_baz(param1, param2): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize("param", [1, 2, 3])
/// def test_foo(param): ...
///
///
/// @pytest.mark.parametrize(("param1", "param2"), [(1, 2), (3, 4)])
/// def test_bar(param1, param2): ...
/// ```
///
/// ## Options
/// - `lint.flake8-pytest-style.parametrize-names-type`
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
#[derive(ViolationMetadata)]
pub(crate) struct PytestParametrizeNamesWrongType {
    single_argument: bool,
    expected: types::ParametrizeNameType,
}

impl Violation for PytestParametrizeNamesWrongType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeNamesWrongType {
            single_argument,
            expected,
        } = self;
        let expected_string = {
            if *single_argument {
                "`str`".to_string()
            } else {
                match expected {
                    types::ParametrizeNameType::Csv => format!("a {expected}"),
                    types::ParametrizeNameType::Tuple | types::ParametrizeNameType::List => {
                        format!("`{expected}`")
                    }
                }
            }
        };
        format!("Wrong type passed to first argument of `pytest.mark.parametrize`; expected {expected_string}")
    }

    fn fix_title(&self) -> Option<String> {
        let PytestParametrizeNamesWrongType {
            single_argument,
            expected,
        } = self;
        let expected_string = {
            if *single_argument {
                "string".to_string()
            } else {
                match expected {
                    types::ParametrizeNameType::Csv => format!("{expected}"),
                    types::ParametrizeNameType::Tuple | types::ParametrizeNameType::List => {
                        format!("`{expected}`")
                    }
                }
            }
        };
        Some(format!("Use a {expected_string} for the first argument"))
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
/// [`lint.flake8-pytest-style.parametrize-values-type`] setting, while the
/// style for each row of values can be configured via the
/// [`lint.flake8-pytest-style.parametrize-values-row-type`] setting.
///
/// For example, [`lint.flake8-pytest-style.parametrize-values-type`] will lead to
/// the following expectations:
///
/// - `tuple`: `@pytest.mark.parametrize("value", ("a", "b", "c"))`
/// - `list`: `@pytest.mark.parametrize("value", ["a", "b", "c"])`
///
/// Similarly, [`lint.flake8-pytest-style.parametrize-values-row-type`] will lead to
/// the following expectations:
///
/// - `tuple`: `@pytest.mark.parametrize(("key", "value"), [("a", "b"), ("c", "d")])`
/// - `list`: `@pytest.mark.parametrize(("key", "value"), [["a", "b"], ["c", "d"]])`
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// # expected list, got tuple
/// @pytest.mark.parametrize("param", (1, 2))
/// def test_foo(param): ...
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
/// def test_bar(param1, param2): ...
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
/// def test_baz(param1, param2): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// @pytest.mark.parametrize("param", [1, 2, 3])
/// def test_foo(param): ...
///
///
/// @pytest.mark.parametrize(("param1", "param2"), [(1, 2), (3, 4)])
/// def test_bar(param1, param2): ...
/// ```
///
/// ## Options
/// - `lint.flake8-pytest-style.parametrize-values-type`
/// - `lint.flake8-pytest-style.parametrize-values-row-type`
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
#[derive(ViolationMetadata)]
pub(crate) struct PytestParametrizeValuesWrongType {
    values: types::ParametrizeValuesType,
    row: types::ParametrizeValuesRowType,
}

impl Violation for PytestParametrizeValuesWrongType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestParametrizeValuesWrongType { values, row } = self;
        format!("Wrong values type in `pytest.mark.parametrize` expected `{values}` of `{row}`")
    }

    fn fix_title(&self) -> Option<String> {
        let PytestParametrizeValuesWrongType { values, row } = self;
        Some(format!("Use `{values}` of `{row}` for parameter values"))
    }
}

/// ## What it does
/// Checks for duplicate test cases in `pytest.mark.parametrize`.
///
/// ## Why is this bad?
/// Duplicate test cases are redundant and should be removed.
///
/// ## Example
///
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
/// def test_foo(param1, param2): ...
/// ```
///
/// Use instead:
///
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
/// def test_foo(param1, param2): ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as tests that rely on mutable global
/// state may be affected by removing duplicate test cases.
///
/// ## References
/// - [`pytest` documentation: How to parametrize fixtures and test functions](https://docs.pytest.org/en/latest/how-to/parametrize.html#pytest-mark-parametrize)
#[derive(ViolationMetadata)]
pub(crate) struct PytestDuplicateParametrizeTestCases {
    index: usize,
}

impl Violation for PytestDuplicateParametrizeTestCases {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestDuplicateParametrizeTestCases { index } = self;
        format!("Duplicate of test case at index {index} in `pytest.mark.parametrize`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate test case".to_string())
    }
}

fn elts_to_csv(elts: &[Expr], generator: Generator, flags: StringLiteralFlags) -> Option<String> {
    if !elts.iter().all(Expr::is_string_literal_expr) {
        return None;
    }

    let node = Expr::from(ast::StringLiteral {
        value: elts
            .iter()
            .fold(String::new(), |mut acc, elt| {
                if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = elt {
                    if !acc.is_empty() {
                        acc.push_str(", ");
                    }
                    acc.push_str(value.to_str());
                }
                acc
            })
            .into_boxed_str(),
        range: TextRange::default(),
        flags,
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
    call: &ExprCall,
    expr: &Expr,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Option<TextRange> {
    parenthesized_range(
        expr.into(),
        (&call.arguments).into(),
        comment_ranges,
        source,
    )
}

/// PT006
fn check_names(checker: &Checker, call: &ExprCall, expr: &Expr, argvalues: &Expr) {
    let names_type = checker.settings.flake8_pytest_style.parametrize_names_type;

    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            let names = split_names(value.to_str());
            if names.len() > 1 {
                match names_type {
                    types::ParametrizeNameType::Tuple => {
                        let name_range = get_parametrize_name_range(
                            call,
                            expr,
                            checker.comment_ranges(),
                            checker.locator().contents(),
                        )
                        .unwrap_or(expr.range());
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
                                expected: names_type,
                            },
                            name_range,
                        );
                        let node = Expr::Tuple(ast::ExprTuple {
                            elts: names
                                .iter()
                                .map(|name| {
                                    Expr::from(ast::StringLiteral {
                                        value: Box::from(*name),
                                        range: TextRange::default(),
                                        flags: checker.default_string_flags(),
                                    })
                                })
                                .collect(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                            parenthesized: true,
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            format!("({})", checker.generator().expr(&node)),
                            name_range,
                        )));
                        checker.report_diagnostic(diagnostic);
                    }
                    types::ParametrizeNameType::List => {
                        let name_range = get_parametrize_name_range(
                            call,
                            expr,
                            checker.comment_ranges(),
                            checker.locator().contents(),
                        )
                        .unwrap_or(expr.range());
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
                                expected: names_type,
                            },
                            name_range,
                        );
                        let node = Expr::List(ast::ExprList {
                            elts: names
                                .iter()
                                .map(|name| {
                                    Expr::from(ast::StringLiteral {
                                        value: Box::from(*name),
                                        range: TextRange::default(),
                                        flags: checker.default_string_flags(),
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
                        checker.report_diagnostic(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {}
                }
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            if elts.len() == 1 {
                handle_single_name(checker, expr, &elts[0], argvalues);
            } else {
                match names_type {
                    types::ParametrizeNameType::Tuple => {}
                    types::ParametrizeNameType::List => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
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
                        checker.report_diagnostic(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if let Some(content) =
                            elts_to_csv(elts, checker.generator(), checker.default_string_flags())
                        {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                content,
                                expr.range(),
                            )));
                        }
                        checker.report_diagnostic(diagnostic);
                    }
                }
            }
        }
        Expr::List(ast::ExprList { elts, .. }) => {
            if elts.len() == 1 {
                handle_single_name(checker, expr, &elts[0], argvalues);
            } else {
                match names_type {
                    types::ParametrizeNameType::List => {}
                    types::ParametrizeNameType::Tuple => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        let node = Expr::Tuple(ast::ExprTuple {
                            elts: elts.clone(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                            parenthesized: true,
                        });
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            format!("({})", checker.generator().expr(&node)),
                            expr.range(),
                        )));
                        checker.report_diagnostic(diagnostic);
                    }
                    types::ParametrizeNameType::Csv => {
                        let mut diagnostic = Diagnostic::new(
                            PytestParametrizeNamesWrongType {
                                single_argument: false,
                                expected: names_type,
                            },
                            expr.range(),
                        );
                        if let Some(content) =
                            elts_to_csv(elts, checker.generator(), checker.default_string_flags())
                        {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                content,
                                expr.range(),
                            )));
                        }
                        checker.report_diagnostic(diagnostic);
                    }
                }
            }
        }
        _ => {}
    }
}

/// PT007
fn check_values(checker: &Checker, names: &Expr, values: &Expr) {
    let values_type = checker.settings.flake8_pytest_style.parametrize_values_type;

    let values_row_type = checker
        .settings
        .flake8_pytest_style
        .parametrize_values_row_type;

    let is_multi_named = if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &names {
        split_names(value.to_str()).len() > 1
    } else {
        true
    };

    match values {
        Expr::List(ast::ExprList { elts, .. }) => {
            if values_type != types::ParametrizeValuesType::List {
                let mut diagnostic = Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    values.range(),
                );
                diagnostic.set_fix({
                    // Determine whether the last element has a trailing comma. Single-element
                    // tuples _require_ a trailing comma, so this is a single-element list
                    // _without_ a trailing comma, we need to insert one.
                    let needs_trailing_comma = if let [item] = elts.as_slice() {
                        SimpleTokenizer::new(
                            checker.locator().contents(),
                            TextRange::new(item.end(), values.end()),
                        )
                        .all(|token| token.kind != SimpleTokenKind::Comma)
                    } else {
                        false
                    };

                    // Replace `[` with `(`.
                    let values_start = Edit::replacement(
                        "(".into(),
                        values.start(),
                        values.start() + TextSize::from(1),
                    );
                    // Replace `]` with `)` or `,)`.
                    let values_end = Edit::replacement(
                        if needs_trailing_comma {
                            ",)".into()
                        } else {
                            ")".into()
                        },
                        values.end() - TextSize::from(1),
                        values.end(),
                    );
                    Fix::unsafe_edits(values_start, [values_end])
                });
                checker.report_diagnostic(diagnostic);
            }

            if is_multi_named {
                handle_value_rows(checker, elts, values_type, values_row_type);
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            if values_type != types::ParametrizeValuesType::Tuple {
                let mut diagnostic = Diagnostic::new(
                    PytestParametrizeValuesWrongType {
                        values: values_type,
                        row: values_row_type,
                    },
                    values.range(),
                );
                diagnostic.set_fix({
                    // Determine whether a trailing comma is present due to the _requirement_
                    // that a single-element tuple must have a trailing comma, e.g., `(1,)`.
                    //
                    // If the trailing comma is on its own line, we intentionally ignore it,
                    // since the expression is already split over multiple lines, as in:
                    // ```python
                    // @pytest.mark.parametrize(
                    //     (
                    //         "x",
                    //     ),
                    // )
                    // ```
                    let has_trailing_comma = elts.len() == 1
                        && checker.locator().up_to(values.end()).chars().rev().nth(1) == Some(',');

                    // Replace `(` with `[`.
                    let values_start = Edit::replacement(
                        "[".into(),
                        values.start(),
                        values.start() + TextSize::from(1),
                    );
                    // Replace `)` or `,)` with `]`.
                    let start = if has_trailing_comma {
                        values.end() - TextSize::from(2)
                    } else {
                        values.end() - TextSize::from(1)
                    };
                    let values_end = Edit::replacement("]".into(), start, values.end());

                    Fix::unsafe_edits(values_start, [values_end])
                });
                checker.report_diagnostic(diagnostic);
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
fn trailing_comma(element: &Expr, source: &str, max_index: TextSize) -> TextSize {
    for token in SimpleTokenizer::starts_at(element.end(), source) {
        if matches!(token.kind, SimpleTokenKind::Comma) {
            return token.start();
        } else if token.start() >= max_index {
            return max_index;
        }
    }
    max_index
}

/// PT014
fn check_duplicates(checker: &Checker, values: &Expr) {
    let (Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) =
        values
    else {
        return;
    };

    let mut seen: FxHashMap<ComparableExpr, usize> =
        FxHashMap::with_capacity_and_hasher(elts.len(), FxBuildHasher);
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
                    let values_end = values.end() - TextSize::new(1);
                    let previous_end =
                        trailing_comma(prev, checker.locator().contents(), values_end);
                    let element_end =
                        trailing_comma(element, checker.locator().contents(), values_end);
                    let deletion_range = TextRange::new(previous_end, element_end);
                    if !checker.comment_ranges().intersects(deletion_range) {
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(deletion_range)));
                    }
                }
                checker.report_diagnostic(diagnostic);
            })
            .or_insert(index);
        prev = Some(element);
    }
}

fn handle_single_name(checker: &Checker, argnames: &Expr, value: &Expr, argvalues: &Expr) {
    let mut diagnostic = Diagnostic::new(
        PytestParametrizeNamesWrongType {
            single_argument: true,
            expected: types::ParametrizeNameType::Csv,
        },
        argnames.range(),
    );
    // If `argnames` and all items in `argvalues` are single-element sequences,
    // they all should be unpacked. Here's an example:
    //
    // ```python
    // @pytest.mark.parametrize(("x",), [(1,), (2,)])
    // def test_foo(x):
    //     assert isinstance(x, int)
    // ```
    //
    // The code above should be transformed into:
    //
    // ```python
    // @pytest.mark.parametrize("x", [1, 2])
    // def test_foo(x):
    //     assert isinstance(x, int)
    // ```
    //
    // Only unpacking `argnames` would break the test:
    //
    // ```python
    // @pytest.mark.parametrize("x", [(1,), (2,)])
    // def test_foo(x):
    //     assert isinstance(x, int)  # fails because `x` is a tuple, not an int
    // ```
    let argvalues_edits = unpack_single_element_items(checker, argvalues);
    let argnames_edit = Edit::range_replacement(checker.generator().expr(value), argnames.range());
    let fix = if checker.comment_ranges().intersects(argnames_edit.range())
        || argvalues_edits
            .iter()
            .any(|edit| checker.comment_ranges().intersects(edit.range()))
    {
        Fix::unsafe_edits(argnames_edit, argvalues_edits)
    } else {
        Fix::safe_edits(argnames_edit, argvalues_edits)
    };
    diagnostic.set_fix(fix);
    checker.report_diagnostic(diagnostic);
}

/// Generate [`Edit`]s to unpack single-element lists or tuples in the given [`Expr`].
/// For instance, `[(1,) (2,)]` will be transformed into `[1, 2]`.
fn unpack_single_element_items(checker: &Checker, expr: &Expr) -> Vec<Edit> {
    let (Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) = expr
    else {
        return vec![];
    };

    let mut edits = Vec::with_capacity(elts.len());
    for value in elts {
        let (Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) =
            value
        else {
            return vec![];
        };

        let [elt] = elts.as_slice() else {
            return vec![];
        };

        if matches!(elt, Expr::Starred(_)) {
            return vec![];
        }

        edits.push(Edit::range_replacement(
            checker.generator().expr(elt),
            value.range(),
        ));
    }
    edits
}

fn handle_value_rows(
    checker: &Checker,
    elts: &[Expr],
    values_type: types::ParametrizeValuesType,
    values_row_type: types::ParametrizeValuesRowType,
) {
    for elt in elts {
        match elt {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                if values_row_type != types::ParametrizeValuesRowType::Tuple {
                    let mut diagnostic = Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        elt.range(),
                    );
                    diagnostic.set_fix({
                        // Determine whether a trailing comma is present due to the _requirement_
                        // that a single-element tuple must have a trailing comma, e.g., `(1,)`.
                        //
                        // If the trailing comma is on its own line, we intentionally ignore it,
                        // since the expression is already split over multiple lines, as in:
                        // ```python
                        // @pytest.mark.parametrize(
                        //     (
                        //         "x",
                        //     ),
                        // )
                        // ```
                        let has_trailing_comma = elts.len() == 1
                            && checker.locator().up_to(elt.end()).chars().rev().nth(1) == Some(',');

                        // Replace `(` with `[`.
                        let elt_start = Edit::replacement(
                            "[".into(),
                            elt.start(),
                            elt.start() + TextSize::from(1),
                        );
                        // Replace `)` or `,)` with `]`.
                        let start = if has_trailing_comma {
                            elt.end() - TextSize::from(2)
                        } else {
                            elt.end() - TextSize::from(1)
                        };
                        let elt_end = Edit::replacement("]".into(), start, elt.end());
                        Fix::unsafe_edits(elt_start, [elt_end])
                    });
                    checker.report_diagnostic(diagnostic);
                }
            }
            Expr::List(ast::ExprList { elts, .. }) => {
                if values_row_type != types::ParametrizeValuesRowType::List {
                    let mut diagnostic = Diagnostic::new(
                        PytestParametrizeValuesWrongType {
                            values: values_type,
                            row: values_row_type,
                        },
                        elt.range(),
                    );
                    diagnostic.set_fix({
                        // Determine whether the last element has a trailing comma. Single-element
                        // tuples _require_ a trailing comma, so this is a single-element list
                        // _without_ a trailing comma, we need to insert one.
                        let needs_trailing_comma = if let [item] = elts.as_slice() {
                            SimpleTokenizer::new(
                                checker.locator().contents(),
                                TextRange::new(item.end(), elt.end()),
                            )
                            .all(|token| token.kind != SimpleTokenKind::Comma)
                        } else {
                            false
                        };

                        // Replace `[` with `(`.
                        let elt_start = Edit::replacement(
                            "(".into(),
                            elt.start(),
                            elt.start() + TextSize::from(1),
                        );
                        // Replace `]` with `)` or `,)`.
                        let elt_end = Edit::replacement(
                            if needs_trailing_comma {
                                ",)".into()
                            } else {
                                ")".into()
                            },
                            elt.end() - TextSize::from(1),
                            elt.end(),
                        );
                        Fix::unsafe_edits(elt_start, [elt_end])
                    });
                    checker.report_diagnostic(diagnostic);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn parametrize(checker: &Checker, call: &ExprCall) {
    if !is_pytest_parametrize(call, checker.semantic()) {
        return;
    }

    if checker.enabled(Rule::PytestParametrizeNamesWrongType) {
        let names = call.arguments.find_argument_value("argnames", 0);
        let values = call.arguments.find_argument_value("argvalues", 1);

        if let (Some(names), Some(values)) = (names, values) {
            check_names(checker, call, names, values);
        }
    }
    if checker.enabled(Rule::PytestParametrizeValuesWrongType) {
        let names = call.arguments.find_argument_value("argnames", 0);
        let values = call.arguments.find_argument_value("argvalues", 1);

        if let (Some(names), Some(values)) = (names, values) {
            check_values(checker, names, values);
        }
    }
    if checker.enabled(Rule::PytestDuplicateParametrizeTestCases) {
        if let Some(values) = call.arguments.find_argument_value("argvalues", 1) {
            check_duplicates(checker, values);
        }
    }
}
