use memchr::memchr2_iter;
use ruff_python_ast::{AstNode, ExprKind, StmtKind};
use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_python_literal::format::FormatSpec;
use ruff_python_parser::{UnsupportedSyntaxErrorKind, parse_expression};
use ruff_python_semantic::analyze::logging::is_logger_candidate;
use ruff_python_semantic::{Modules, SemanticModel, TypingOnlyBindingsStatus};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::fastapi::rules::is_fastapi_route_call;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Searches for strings that look like they were meant to be f-strings, but are missing an `f` prefix.
///
/// ## Why is this bad?
/// Expressions inside curly braces are only evaluated if the string has an `f` prefix.
///
/// ## Details
///
/// There are many possible string literals which are not meant to be f-strings
/// despite containing f-string-like syntax. As such, this lint ignores all strings
/// where one of the following conditions applies:
///
/// 1. The string is a standalone expression. For example, the rule ignores all docstrings.
/// 2. The string is part of a function call with argument names that match at least one variable
///    (for example: `format("Message: {value}", value="Hello World")`)
/// 3. The string (or a parent expression of the string) has a direct method call on it
///    (for example: `"{value}".format(...)`)
/// 4. The string has no `{...}` expression sections, or uses invalid f-string syntax.
/// 5. The string references variables that are not in scope, or it doesn't capture variables at all.
/// 6. Any format specifiers in the potential f-string are invalid.
/// 7. The string is part of a function call that is known to expect a template string rather than an
///    evaluated f-string: for example, a [`logging`][logging] call, a [`gettext`][gettext] call,
///    or a [FastAPI path].
///
/// ## Example
///
/// ```python
/// name = "Sarah"
/// day_of_week = "Tuesday"
/// print("Hello {name}! It is {day_of_week} today!")
/// ```
///
/// Use instead:
/// ```python
/// name = "Sarah"
/// day_of_week = "Tuesday"
/// print(f"Hello {name}! It is {day_of_week} today!")
/// ```
///
/// ## Fix safety
///
/// This fix will always change the behavior of the program and, despite the precautions detailed
/// above, this may be undesired. As such the fix is always marked as unsafe.
///
/// ## Options
///
/// - `lint.logger-objects`
///
/// [logging]: https://docs.python.org/3/howto/logging-cookbook.html#using-particular-formatting-styles-throughout-your-application
/// [gettext]: https://docs.python.org/3/library/gettext.html
/// [FastAPI path]: https://fastapi.tiangolo.com/tutorial/path-params/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.2.1", category = Category::Suspicious)]
pub(crate) struct MissingFStringSyntax;

impl AlwaysFixableViolation for MissingFStringSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        r"Possible f-string without an `f` prefix".to_string()
    }

    fn fix_title(&self) -> String {
        "Add `f` prefix".into()
    }
}

/// RUF027
pub(crate) fn missing_fstring_syntax(checker: &Checker, literal: ast::StringLiteral<'_>) {
    let semantic = checker.semantic();

    // fstrings are never correct as type definitions
    if semantic.in_type_definition() {
        return;
    }

    // we want to avoid statement expressions that are just a string literal.
    // there's no reason to have standalone f-strings and this lets us avoid docstrings too
    if let StmtKind::Expr(expr_node) = semantic.current_statement().classify() {
        let value = expr_node.value();
        match value.classify() {
            ExprKind::StringLiteral(_) | ExprKind::FString(_) => return,
            _ => {}
        }
    }

    let logger_objects = &checker.settings().logger_objects;
    let fastapi_seen = semantic.seen_module(Modules::FASTAPI);

    // We also want to avoid:
    // - Expressions inside `gettext()` calls
    // - Expressions passed to logging calls (since the `logging` module evaluates them lazily:
    //   https://docs.python.org/3/howto/logging-cookbook.html#using-particular-formatting-styles-throughout-your-application)
    // - `fastAPI` paths: https://fastapi.tiangolo.com/tutorial/path-params/
    // - Expressions where a method is immediately called on the string literal
    if semantic
        .current_expressions()
        .filter_map(ast::Expr::as_call_expr)
        .any(|call_expr| {
            is_method_call_on_literal(call_expr, literal)
                || is_gettext(call_expr, semantic)
                || is_logger_candidate(call_expr.func(), semantic, logger_objects)
                || (fastapi_seen && is_fastapi_route_call(call_expr, semantic))
        })
    {
        return;
    }

    if should_be_fstring(
        literal,
        checker.locator(),
        semantic,
        checker.target_version(),
    ) {
        checker
            .report_diagnostic(MissingFStringSyntax, literal.range())
            .set_fix(fix_fstring_syntax(literal.range()));
    }
}

/// Returns `true` if an expression appears to be a `gettext` call.
///
/// We want to avoid statement expressions and assignments related to aliases
/// of the gettext API.
///
/// See <https://docs.python.org/3/library/gettext.html> for details. When one
/// uses `_` to mark a string for translation, the tools look for these markers
/// and replace the original string with its translated counterpart. If the
/// string contains variable placeholders or formatting, it can complicate the
/// translation process, lead to errors or incorrect translations.
fn is_gettext(call_expr: ast::ExprCall<'_>, semantic: &SemanticModel) -> bool {
    let func = call_expr.func();
    let short_circuit = match func.classify() {
        ExprKind::Name(expr_name) => {
            let id = expr_name.id();

            matches!(id.as_str(), "gettext" | "ngettext" | "_")
        }
        ExprKind::Attribute(expr_attribute) => {
            let attr = expr_attribute.attr();

            matches!(attr.as_str(), "gettext" | "ngettext")
        }
        _ => false,
    };

    if short_circuit {
        return true;
    }

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["gettext", "gettext" | "ngettext"] | ["builtins", "_"]
            )
        })
}

/// Return `true` if `call_expr` is a method call on an [`ast::ExprStringLiteral`]
/// in which `literal` is one of the [`ast::StringLiteral`] parts.
///
/// For example: `expr` is a node representing the expression `"{foo}".format(foo="bar")`,
/// and `literal` is the node representing the string literal `"{foo}"`.
fn is_method_call_on_literal(
    call_expr: ast::ExprCall<'_>,
    literal: ast::StringLiteral<'_>,
) -> bool {
    let ExprKind::Attribute(attribute) = call_expr.func().classify() else {
        return false;
    };
    let value = attribute.value();
    let ExprKind::StringLiteral(string_literal) = value.classify() else {
        return false;
    };
    let value = string_literal.value();
    value.iter().any(|part| part.is_same_node(literal))
}

/// Returns `true` if `literal` is likely an f-string with a missing `f` prefix.
/// See [`MissingFStringSyntax`] for the validation criteria.
fn should_be_fstring(
    literal: ast::StringLiteral<'_>,
    locator: &Locator,
    semantic: &SemanticModel,
    target_version: PythonVersion,
) -> bool {
    if !has_brackets(literal.value()) {
        return false;
    }

    let fstring_expr = format!("f{}", locator.slice(literal));
    let Ok(parsed) = parse_expression(&fstring_expr) else {
        return false;
    };

    // For Python < 3.12, reject if the parser detected any PEP 701 f-string
    // features.
    if target_version < PythonVersion::PY312 {
        let has_pep701 = parsed
            .unsupported_syntax_errors()
            .iter()
            .any(|e| matches!(e.kind, UnsupportedSyntaxErrorKind::Pep701FString(_)));
        if has_pep701 {
            return false;
        }
    }

    // Note: Range offsets for `value` are based on `fstring_expr`
    let ExprKind::FString(f_string_node) = parsed.expr().classify() else {
        return false;
    };
    let value = f_string_node.value();

    let mut arg_names = FxHashSet::default();
    for expr in semantic
        .current_expressions()
        .filter_map(ast::Expr::as_call_expr)
    {
        let arguments_node = expr.arguments();
        let keywords = arguments_node.keywords();
        let args = arguments_node.args();
        for keyword in keywords {
            if let Some(ident) = keyword.arg() {
                arg_names.insert(ident.id());
            }
        }
        for arg in args {
            if let ExprKind::Name(name_node) = arg.classify() {
                let id = name_node.id();
                arg_names.insert(id);
            }
        }
    }

    for f_string in value.f_strings() {
        let mut has_name = false;
        for element in f_string.elements().interpolations() {
            if let ExprKind::Name(name_node) = element.expression().classify() {
                let id = name_node.id();
                if arg_names.contains(id) {
                    return false;
                }
                if semantic
                    // the parsed expression nodes have incorrect ranges
                    // so we need to use the range of the literal for the
                    // lookup in order to get reasonable results.
                    .simulate_runtime_load_at_location_in_scope(
                        id,
                        literal.range(),
                        semantic.scope_id,
                        TypingOnlyBindingsStatus::Disallowed,
                    )
                    .is_none_or(|id| semantic.binding(id).kind.is_builtin())
                {
                    return false;
                }
                has_name = true;
            }
            if let Some(spec) = element.format_spec() {
                let spec = &fstring_expr[spec.range()];
                if FormatSpec::parse(spec).is_err() {
                    return false;
                }
            }
        }
        if !has_name {
            return false;
        }
    }

    true
}

// fast check to disqualify any string literal without brackets
#[inline]
fn has_brackets(possible_fstring: &str) -> bool {
    // this qualifies rare false positives like "{ unclosed bracket"
    // but it's faster in the general case
    memchr2_iter(b'{', b'}', possible_fstring.as_bytes())
        .nth(1)
        .is_some()
}

fn fix_fstring_syntax(range: TextRange) -> Fix {
    Fix::unsafe_edit(Edit::insertion("f".into(), range.start()))
}
