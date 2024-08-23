use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_literal::format::FormatSpec;
use ruff_python_parser::parse_expression;
use ruff_python_semantic::analyze::logging::is_logger_candidate;
use ruff_python_semantic::{Binding, Modules, ScopeId, SemanticModel};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use memchr::memchr2_iter;
use rustc_hash::FxHashSet;

use crate::checkers::ast::Checker;
use crate::rules::fastapi::rules::is_fastapi_route_call;

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
///    evaluated f-string: for example, a [`logging`] call, a [`gettext`] call, or a [`fastAPI` path].
/// 8. The string is assigned to a symbol where any of the *references* to that symbol are part of a
///    function call that is known to expect a template string rather than an evaluated f-string.
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
/// [`logging`]: https://docs.python.org/3/howto/logging-cookbook.html#using-particular-formatting-styles-throughout-your-application
/// [`gettext`]: https://docs.python.org/3/library/gettext.html
/// [`fastAPI` path]: https://fastapi.tiangolo.com/tutorial/path-params/
#[violation]
pub struct MissingFStringSyntax;

impl AlwaysFixableViolation for MissingFStringSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible f-string without an `f` prefix")
    }

    fn fix_title(&self) -> String {
        "Add `f` prefix".to_string()
    }
}

/// RUF027
///
/// Analyze a [`ast::StringLiteral`] node to see if it looks like it could be a string that was
/// meant to be an f-string, but is missing an `f` prefix. If so, emit a diagnostic.
///
/// This routine skips any string literals that are part of "simple assignments", e.g. `x = "foo"`
/// or `x: str = "foo"`. These are checked by the [`missing_fstring_syntax_binding`] function
/// below, which is part of the same check.
pub(crate) fn missing_fstring_syntax_expr(checker: &mut Checker, literal: &ast::StringLiteral) {
    if !has_brackets(&literal.value) {
        return;
    }

    let semantic = checker.semantic();

    // we want to avoid statement expressions that are just a string literal.
    // there's no reason to have standalone f-strings and this lets us avoid docstrings too
    if let ast::Stmt::Expr(ast::StmtExpr { value, .. }) = semantic.current_statement() {
        match value.as_ref() {
            ast::Expr::StringLiteral(_) | ast::Expr::FString(_) => return,
            _ => {}
        }
    }

    // Simple assignments will be dealt with by `missing_fstring_syntax_binding`
    if semantic
        .current_statements()
        .any(|stmt| is_simple_assignment_to_literal(stmt, literal))
    {
        return;
    }

    let logger_objects = &checker.settings.logger_objects;
    let fastapi_seen = semantic.seen_module(Modules::FASTAPI);

    let mut arg_names = FxHashSet::default();

    // We also want to avoid:
    // - Expressions inside `gettext()` calls
    // - Expressions passed to logging calls (since the `logging` module evaluates them lazily:
    //   https://docs.python.org/3/howto/logging-cookbook.html#using-particular-formatting-styles-throughout-your-application)
    // - `fastAPI` paths: https://fastapi.tiangolo.com/tutorial/path-params/
    // - Expressions where a method is immediately called on the string literal
    for call_expr in semantic
        .current_expressions()
        .filter_map(ast::Expr::as_call_expr)
    {
        if is_method_call_on_literal(call_expr, literal) {
            return;
        }
        if is_gettext(call_expr, semantic) {
            return;
        }
        if is_logger_candidate(&call_expr.func, semantic, logger_objects) {
            return;
        }
        if fastapi_seen && is_fastapi_route_call(call_expr, semantic) {
            return;
        }
        let ast::Arguments { keywords, args, .. } = &call_expr.arguments;
        for keyword in &**keywords {
            if let Some(ident) = keyword.arg.as_ref() {
                arg_names.insert(&ident.id);
            }
        }
        for arg in &**args {
            if let ast::Expr::Name(ast::ExprName { id, .. }) = arg {
                arg_names.insert(id);
            }
        }
    }

    if should_be_fstring(
        literal,
        checker.locator(),
        semantic,
        semantic.scope_id,
        &arg_names,
    ) {
        let diagnostic = Diagnostic::new(MissingFStringSyntax, literal.range())
            .with_fix(fix_fstring_syntax(literal.range()));
        checker.diagnostics.push(diagnostic);
    }
}

/// RUF027
///
/// Analyze a [`Binding`] to see if the bound variable is a string literal
/// that's part of a "simple assignment", e.g. `x = "foo"` or `x: str = "foo"`.
/// If it is, check to see if the string looks like it's meant to be an f-string,
/// but is missing an `f` prefix. False positives are minimized by analyzing the references
/// to the binding, to see if the binding is ever used in a way that indicates that the
/// string is clearly meant to be a string template rather than an f-string.
pub(crate) fn missing_fstring_syntax_binding(
    checker: &Checker,
    binding: &Binding,
) -> Option<Diagnostic> {
    let semantic = checker.semantic();
    let locator = checker.locator();

    let stmt = binding.statement(semantic)?;
    let string_literal = match stmt {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => match targets.as_slice() {
            [_] => value.as_string_literal_expr()?,
            _ => return None,
        },
        ast::Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => value.as_string_literal_expr()?,
        _ => return None,
    };
    let [string_literal] = string_literal.value.as_slice() else {
        return None;
    };
    if !has_brackets(&string_literal.value) {
        return None;
    }

    let logger_objects = &checker.settings.logger_objects;
    let mut arg_names = FxHashSet::default();

    for reference in binding.references().map(|id| semantic.reference(id)) {
        let Some(expr_id) = reference.expression_id() else {
            continue;
        };
        for call_expr in semantic
            .expressions(expr_id)
            .filter_map(ast::Expr::as_call_expr)
        {
            if is_method_call_on_name(call_expr, binding.name(locator)) {
                return None;
            }
            if is_gettext(call_expr, semantic) {
                return None;
            }
            if is_logger_candidate(&call_expr.func, semantic, logger_objects) {
                return None;
            }
            let ast::Arguments { keywords, args, .. } = &call_expr.arguments;
            for keyword in &**keywords {
                if let Some(ident) = keyword.arg.as_ref() {
                    arg_names.insert(&ident.id);
                }
            }
            for arg in &**args {
                if let ast::Expr::Name(ast::ExprName { id, .. }) = arg {
                    arg_names.insert(id);
                }
            }
        }
    }

    should_be_fstring(string_literal, locator, semantic, binding.scope, &arg_names).then(|| {
        Diagnostic::new(MissingFStringSyntax, string_literal.range())
            .with_fix(fix_fstring_syntax(string_literal.range()))
    })
}

/// Determine whether `stmt` represents a "simple assignment" to the string literal `literal`.
///
/// For our purposes here, a "simple assignment" is either an annotated assignment (`foo: str = "bar"`)
/// or an unannotated assignment with a single target (`foo = "bar"`).
fn is_simple_assignment_to_literal(stmt: &ast::Stmt, literal: &ast::StringLiteral) -> bool {
    match stmt {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => match targets.as_slice() {
            [_] => value.range() == literal.range(),
            _ => false,
        },
        ast::Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => value.range() == literal.range(),
        _ => false,
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
fn is_gettext(call_expr: &ast::ExprCall, semantic: &SemanticModel) -> bool {
    let func = &*call_expr.func;
    let short_circuit = match func {
        ast::Expr::Name(ast::ExprName { id, .. }) => {
            matches!(id.as_str(), "gettext" | "ngettext" | "_")
        }
        ast::Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
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
fn is_method_call_on_literal(call_expr: &ast::ExprCall, literal: &ast::StringLiteral) -> bool {
    let ast::Expr::Attribute(ast::ExprAttribute { value, .. }) = &*call_expr.func else {
        return false;
    };
    let ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &**value else {
        return false;
    };
    value.as_slice().contains(literal)
}

/// Determine whether `call_expr` represents a method call on a symbol bound to the name `name`.
///
/// E.g., `foo.format(bar=3)`, where `name == "foo"`.
fn is_method_call_on_name(call_expr: &ast::ExprCall, name: &str) -> bool {
    let ast::Expr::Attribute(ast::ExprAttribute { value, .. }) = &*call_expr.func else {
        return false;
    };
    let ast::Expr::Name(ast::ExprName { id, .. }) = &**value else {
        return false;
    };
    id == name
}

/// Returns `true` if `literal` is likely an f-string with a missing `f` prefix.
/// See [`MissingFStringSyntax`] for the validation criteria.
fn should_be_fstring(
    literal: &ast::StringLiteral,
    locator: &Locator,
    semantic: &SemanticModel,
    scope: ScopeId,
    relevant_argument_names: &FxHashSet<&ast::name::Name>,
) -> bool {
    let fstring_expr = format!("f{}", locator.slice(literal));
    let Ok(parsed) = parse_expression(&fstring_expr) else {
        return false;
    };

    // Note: Range offsets for `value` are based on `fstring_expr`
    let ast::Expr::FString(ast::ExprFString { value, .. }) = parsed.expr() else {
        return false;
    };

    for f_string in value.f_strings() {
        let mut has_name = false;
        for element in f_string.elements.expressions() {
            if let ast::Expr::Name(ast::ExprName { id, .. }) = element.expression.as_ref() {
                if relevant_argument_names.contains(id) {
                    return false;
                }
                let Some(binding_id) = semantic.lookup_symbol_in_scope(id, scope, false) else {
                    return false;
                };
                let binding = semantic.binding(binding_id);
                if binding.kind.is_builtin() {
                    return false;
                }
                if binding.start() > literal.end() {
                    return false;
                }
                has_name = true;
            }
            if let Some(spec) = &element.format_spec {
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
