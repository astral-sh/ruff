use itertools::{Either, Itertools};
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, ArgOrKeyword, BuildId, Builder, Expr, NodeList, Stmt, StmtKind, build,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;

use crate::codes::Category;
use crate::fix::syntax;
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
    fn from_expr_name(func: Expr<'_>, semantic: &SemanticModel) -> Option<Self> {
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
pub(crate) fn legacy_raises_warns_deprecated_call(checker: &Checker, call: ast::ExprCall<'_>) {
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

    if let StmtKind::With(with_node) = semantic.current_statement().classify() {
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
    if has_leading_content(stmt.start(), checker.source())
        || has_trailing_content(stmt.end(), checker.source())
    {
        return;
    }
    let mut builder = Builder::default();
    let Some(with_stmt) = try_fix_legacy_call(&mut builder, context_type, stmt, semantic) else {
        return;
    };
    let generated = checker.generator().stmt(builder.get(with_stmt));
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

fn try_fix_legacy_call(
    builder: &mut Builder,
    context_type: PytestContextType,
    stmt: Stmt<'_>,
    semantic: &SemanticModel,
) -> Option<BuildId<ast::kind::Stmt>> {
    match stmt.classify() {
        StmtKind::Expr(stmt_expr) => {
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
                generate_with_statement(builder, context_type, call, None, None, None)
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
                generate_with_statement(
                    builder,
                    context_type,
                    inner_raises_call,
                    match_arg,
                    None,
                    None,
                )
            } else {
                None
            }
        }
        StmtKind::Assign(stmt_assign) => {
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

            generate_with_statement(
                builder,
                context_type,
                call,
                None,
                optional_vars,
                assign_targets,
            )
        }
        _ => None,
    }
}

fn generate_with_statement(
    builder: &mut Builder,
    context_type: PytestContextType,
    legacy_call: ast::ExprCall<'_>,
    match_arg: Option<Expr<'_>>,
    optional_vars: Option<Expr<'_>>,
    assign_targets: Option<NodeList<'_, ast::kind::Expr>>,
) -> Option<BuildId<ast::kind::Stmt>> {
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
            ArgOrKeyword::Arg(expr) => Either::Left(builder.copy_node(expr)),
            ArgOrKeyword::Keyword(keyword) => Either::Right(builder.copy_node(keyword)),
        });

    let context_func = builder.copy_node(legacy_call.func());
    let args = builder.copy_nodes(expected);
    let keywords = builder.map_list(match_arg, |builder, expr| {
        let arg = syntax::identifier(builder, "match");
        let value = builder.copy_node(expr);
        // The generator orders arguments by range. Keep `match` after the exception.
        builder.add(build::Keyword {
            range: expr.range(),
            arg: Some(arg),
            value,
        })
    });
    let context_expr = syntax::call_with_keywords(builder, context_func, args, keywords);
    let func = builder.copy_node(func);
    let args = builder.list(func_args);
    let keywords = builder.list(func_keywords);
    let call = syntax::call_with_keywords(builder, func, args, keywords);
    let statement = if let Some(targets) = assign_targets {
        let targets = builder.copy_nodes(targets);
        builder
            .add(build::StmtAssign {
                range: TextRange::default(),
                targets,
                value: call,
            })
            .into()
    } else {
        syntax::expression_statement(builder, call)
    };
    let optional_vars = optional_vars.map(|var| builder.copy_node(var));
    let item = builder.add(build::WithItem {
        range: TextRange::default(),
        context_expr,
        optional_vars,
    });
    let items = builder.list([item]);
    let body = builder.list([statement]);
    Some(
        builder
            .add(build::StmtWith {
                range: TextRange::default(),
                is_async: false,
                items,
                body,
            })
            .into(),
    )
}
