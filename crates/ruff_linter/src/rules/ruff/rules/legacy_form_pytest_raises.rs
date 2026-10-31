use itertools::{Either, Itertools};
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::{self as ast, Expr, NodeIndex, Stmt, StmtExpr, StmtWith, WithItem};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;

use crate::codes::Category;
use crate::{FixAvailability, Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for non-context-manager use of `pytest.raises`, `pytest.warns`, and `pytest.deprecated_call`.
///
/// ## Why is this bad?
/// The context-manager form is more readable, easier to extend, and supports additional keyword
/// arguments.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// excinfo = pytest.raises(ValueError, int, "hello")
/// pytest.warns(UserWarning, my_function, arg)
/// pytest.deprecated_call(my_deprecated_function, arg1, arg2)
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// with pytest.raises(ValueError) as excinfo:
///     int("hello")
/// with pytest.warns(UserWarning):
///     my_function(arg)
/// with pytest.deprecated_call():
///     my_deprecated_function(arg1, arg2)
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
/// - [`pytest` documentation: `pytest.warns`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-warns)
/// - [`pytest` documentation: `pytest.deprecated_call`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-deprecated-call)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.15.0", category = Category::Style)]
pub(crate) struct LegacyFormPytestRaises {
    context_type: PytestContextType,
}

impl Violation for LegacyFormPytestRaises {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use context-manager form of `pytest.{}()`",
            self.context_type
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Use `pytest.{}()` as a context-manager",
            self.context_type
        ))
    }
}

/// Enum representing the type of pytest context manager
#[derive(PartialEq, Clone, Copy)]
enum PytestContextType {
    Raises,
    Warns,
    DeprecatedCall,
}

impl fmt::Display for PytestContextType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Raises => "raises",
            Self::Warns => "warns",
            Self::DeprecatedCall => "deprecated_call",
        };
        write!(f, "{name}")
    }
}

impl PytestContextType {
    fn from_expr_name(func: flat::View<'_, Expr>, semantic: &SemanticModel) -> Option<Self> {
        semantic
            .resolve_qualified_name(func)
            .and_then(|qualified_name| match qualified_name.segments() {
                ["pytest", "raises"] => Some(Self::Raises),
                ["pytest", "warns"] => Some(Self::Warns),
                ["pytest", "deprecated_call"] => Some(Self::DeprecatedCall),
                _ => None,
            })
    }

    fn expected_arg(self) -> Option<(&'static str, usize)> {
        match self {
            Self::Raises => Some(("expected_exception", 0)),
            Self::Warns => Some(("expected_warning", 0)),
            Self::DeprecatedCall => None,
        }
    }

    fn func_arg(self) -> (&'static str, usize) {
        match self {
            Self::Raises | Self::Warns => ("func", 1),
            Self::DeprecatedCall => ("func", 0),
        }
    }
}

/// RUF061
pub(crate) fn legacy_raises_warns_deprecated_call(
    checker: &Checker,
    call: flat::View<'_, ast::ExprCall>,
) {
    let semantic = checker.semantic();
    let Some(context_type) = PytestContextType::from_expr_name(call.func(), semantic) else {
        return;
    };

    let (func_arg_name, func_arg_position) = context_type.func_arg();
    if call
        .arguments()
        .find_argument(func_arg_name, func_arg_position)
        .is_none()
    {
        return;
    }

    if let flat::StmtKind::With(with_node) = semantic.current_statement().classify() {
        let items = with_node.items();
        if items
            .iter()
            .any(|item| item.context_expr().as_call_expr() == Some(call))
        {
            return;
        }
    }

    let mut diagnostic =
        checker.report_diagnostic(LegacyFormPytestRaises { context_type }, call.range());

    let stmt = semantic.current_statement();
    if !has_leading_content(stmt.start(), checker.source())
        && !has_trailing_content(stmt.end(), checker.source())
    {
        if let Some(with_stmt) = try_fix_legacy_call(context_type, stmt, semantic) {
            let generated = checker.generator().stmt(&Stmt::With(with_stmt));
            let first_line = checker.locator().line_str(stmt.start());
            let indentation = leading_indentation(first_line);
            let mut indented = String::new();
            for (idx, line) in generated.universal_newlines().enumerate() {
                if idx == 0 {
                    indented.push_str(&line);
                } else {
                    indented.push_str(checker.stylist().line_ending().as_str());
                    indented.push_str(indentation);
                    indented.push_str(&line);
                }
            }

            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                indented,
                stmt.range(),
            )));
        }
    }
}

fn try_fix_legacy_call(
    context_type: PytestContextType,
    stmt: flat::View<'_, Stmt>,
    semantic: &SemanticModel,
) -> Option<StmtWith> {
    match stmt.classify() {
        flat::StmtKind::Expr(stmt_expr) => {
            let value = stmt_expr.value();

            let call = value.as_call_expr()?;

            // Handle two patterns for legacy calls:
            // 1. Direct usage: `pytest.raises(ZeroDivisionError, func, 1, b=0)`
            // 2. With match method: `pytest.raises(ZeroDivisionError, func, 1, b=0).match("division by zero")`
            //
            // The second branch specifically looks for raises().match() pattern which only exists for
            // `raises` (not `warns`/`deprecated_call`) since only `raises` returns an ExceptionInfo
            // object with a .match() method. We need to preserve this match condition when converting
            // to context manager form.
            if PytestContextType::from_expr_name(call.func(), semantic) == Some(context_type) {
                generate_with_statement(context_type, call, None, None, None)
            } else if let PytestContextType::Raises = context_type {
                let inner_raises_call = call
                    .func()
                    .as_attribute_expr()
                    .filter(|expr_attribute| expr_attribute.attr() == "match")
                    .and_then(|expr_attribute| expr_attribute.value().as_call_expr())
                    .filter(|inner_call| {
                        PytestContextType::from_expr_name(inner_call.func(), semantic)
                            == Some(PytestContextType::Raises)
                    })?;
                let match_arg = call.arguments().args().first();
                generate_with_statement(context_type, inner_raises_call, match_arg, None, None)
            } else {
                None
            }
        }
        flat::StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            let value = stmt_assign.value();

            let call = value.as_call_expr().filter(|call| {
                PytestContextType::from_expr_name(call.func(), semantic) == Some(context_type)
            })?;
            let (optional_vars, assign_targets) = match context_type {
                PytestContextType::Raises => {
                    let [target] = targets.as_array()?;
                    (Some(target), None)
                }
                PytestContextType::Warns | PytestContextType::DeprecatedCall => {
                    (None, Some(targets))
                }
            };

            generate_with_statement(context_type, call, None, optional_vars, assign_targets)
        }
        _ => None,
    }
}

fn generate_with_statement(
    context_type: PytestContextType,
    legacy_call: flat::View<'_, ast::ExprCall>,
    match_arg: Option<flat::View<'_, Expr>>,
    optional_vars: Option<flat::View<'_, Expr>>,
    assign_targets: Option<flat::NodeList<'_, Expr>>,
) -> Option<StmtWith> {
    let expected = if let Some((name, position)) = context_type.expected_arg() {
        Some(
            legacy_call
                .arguments()
                .find_argument_value(name, position)?,
        )
    } else {
        None
    };

    let (func_arg_name, func_arg_position) = context_type.func_arg();
    let func = legacy_call
        .arguments()
        .find_argument_value(func_arg_name, func_arg_position)?;

    let (func_args, func_keywords): (Vec<_>, Vec<_>) = legacy_call
        .arguments()
        .iter_source_order()
        .skip(if expected.is_some() { 2 } else { 1 })
        .partition_map(|arg_or_keyword| match arg_or_keyword {
            flat::ArgOrKeyword::Arg(expr) => Either::Left(expr.to_owned_node()),
            flat::ArgOrKeyword::Keyword(keyword) => Either::Right(keyword.to_owned_node()),
        });

    let context_call = ast::ExprCall {
        node_index: NodeIndex::NONE,
        range_start: ruff_text_size::TextSize::default(),
        func: Box::new(legacy_call.func().to_owned_node()),
        arguments: ast::Arguments {
            node_index: NodeIndex::NONE,
            range: TextRange::default(),
            args: expected
                .map(flat::View::to_owned_node)
                .into_iter()
                .collect(),
            keywords: match_arg
                .map(|expr| ast::Keyword {
                    node_index: NodeIndex::NONE,
                    // Take range from the original expression so that the keyword
                    // argument is generated after positional arguments
                    range: expr.range(),
                    arg: Some(ast::Identifier::new("match", TextRange::default())),
                    value: expr.to_owned_node(),
                })
                .into_iter()
                .collect(),
        },
    };

    let func_call = ast::ExprCall {
        node_index: NodeIndex::NONE,
        range_start: ruff_text_size::TextSize::default(),
        func: Box::new(func.to_owned_node()),
        arguments: ast::Arguments {
            node_index: NodeIndex::NONE,
            range: TextRange::default(),
            args: func_args.into(),
            keywords: func_keywords.into(),
        },
    };

    let body = if let Some(assign_targets) = assign_targets {
        Stmt::Assign(ast::StmtAssign {
            node_index: NodeIndex::NONE,
            range: TextRange::default(),
            targets: assign_targets
                .iter()
                .map(flat::View::to_owned_node)
                .collect(),
            value: Box::new(func_call.into()),
        })
    } else {
        Stmt::Expr(StmtExpr {
            node_index: NodeIndex::NONE,
            range: TextRange::default(),
            value: Box::new(func_call.into()),
        })
    };

    Some(StmtWith {
        node_index: NodeIndex::NONE,
        range: TextRange::default(),
        is_async: false,
        items: vec![WithItem {
            node_index: NodeIndex::NONE,
            range: TextRange::default(),
            context_expr: context_call.into(),
            optional_vars: optional_vars.map(|var| Box::new(var.to_owned_node())),
        }],
        body: ast::Suite::from([body]),
    })
}
