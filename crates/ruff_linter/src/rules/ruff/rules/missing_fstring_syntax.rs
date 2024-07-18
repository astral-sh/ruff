use memchr::memchr2_iter;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_literal::format::FormatSpec;
use ruff_python_parser::parse_expression;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

/// ## What it does
/// Checks for strings that contain f-string syntax but are not f-strings.
///
/// ## Why is this bad?
/// An f-string missing an `f` at the beginning won't format anything, and instead
/// treat the interpolation syntax as literal.
///
/// Since there are many possible string literals which contain syntax similar to f-strings yet are not intended to be,
/// this lint will disqualify any literal that satisfies any of the following conditions:
///
/// 1. The string literal is a standalone expression. For example, a docstring.
/// 2. The literal is part of a function call with argument names that match at least one variable (for example: `format("Message: {value}", value = "Hello World")`)
/// 3. The literal (or a parent expression of the literal) has a direct method call on it (for example: `"{value}".format(...)`)
/// 4. The string has no `{...}` expression sections, or uses invalid f-string syntax.
/// 5. The string references variables that are not in scope, or it doesn't capture variables at all.
/// 6. Any format specifiers in the potential f-string are invalid.
///
/// ## Example
///
/// ```python
/// name = "Sarah"
/// dayofweek = "Tuesday"
/// msg = "Hello {name}! It is {dayofweek} today!"
/// ```
///
/// Use instead:
/// ```python
/// name = "Sarah"
/// dayofweek = "Tuesday"
/// msg = f"Hello {name}! It is {dayofweek} today!"
/// ```
#[violation]
pub struct MissingFStringSyntax;

impl AlwaysFixableViolation for MissingFStringSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Possible f-string without an `f` prefix"#)
    }

    fn fix_title(&self) -> String {
        "Add `f` prefix".into()
    }
}

/// RUF027
pub(crate) fn missing_fstring_syntax(
    diagnostics: &mut Vec<Diagnostic>,
    literal: &ast::StringLiteral,
    locator: &Locator,
    semantic: &SemanticModel,
) {
    // we want to avoid statement expressions that are just a string literal.
    // there's no reason to have standalone f-strings and this lets us avoid docstrings too
    if let ast::Stmt::Expr(ast::StmtExpr { value, .. }) = semantic.current_statement() {
        match value.as_ref() {
            ast::Expr::StringLiteral(_) | ast::Expr::FString(_) => return,
            _ => {}
        }
    }

    // We also want to avoid expressions that are intended to be translated.
    if semantic
        .current_expressions()
        .any(|expr| is_gettext(expr, semantic))
    {
        return;
    }

    if should_be_fstring(literal, locator, semantic) {
        let diagnostic = Diagnostic::new(MissingFStringSyntax, literal.range())
            .with_fix(fix_fstring_syntax(literal.range()));
        diagnostics.push(diagnostic);
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
fn is_gettext(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    let ast::Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };

    let short_circuit = match func.as_ref() {
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
                ["gettext", "gettext" | "ngettext"]
            )
        })
}

/// Returns `true` if `literal` is likely an f-string with a missing `f` prefix.
/// See [`MissingFStringSyntax`] for the validation criteria.
fn should_be_fstring(
    literal: &ast::StringLiteral,
    locator: &Locator,
    semantic: &SemanticModel,
) -> bool {
    if !has_brackets(&literal.value) {
        return false;
    }

    let fstring_expr = format!("f{}", locator.slice(literal));
    let Ok(parsed) = parse_expression(&fstring_expr) else {
        return false;
    };

    // Note: Range offsets for `value` are based on `fstring_expr`
    let ast::Expr::FString(ast::ExprFString { value, .. }) = parsed.expr() else {
        return false;
    };

    let mut arg_names = FxHashSet::default();
    let mut last_expr: Option<&ast::Expr> = None;
    for expr in semantic.current_expressions() {
        match expr {
            ast::Expr::Call(ast::ExprCall {
                arguments: ast::Arguments { keywords, args, .. },
                func,
                ..
            }) => {
                if let ast::Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() {
                    match value.as_ref() {
                        // if the first part of the attribute is the string literal,
                        // we want to ignore this literal from the lint.
                        // for example: `"{x}".some_method(...)`
                        ast::Expr::StringLiteral(expr_literal)
                            if expr_literal.value.as_slice().contains(literal) =>
                        {
                            return false;
                        }
                        // if the first part of the attribute was the expression we
                        // just went over in the last iteration, then we also want to pass
                        // this over in the lint.
                        // for example: `some_func("{x}").some_method(...)`
                        value if last_expr == Some(value) => {
                            return false;
                        }
                        _ => {}
                    }
                }
                for keyword in keywords.iter() {
                    if let Some(ident) = keyword.arg.as_ref() {
                        arg_names.insert(ident.as_str());
                    }
                }
                for arg in args.iter() {
                    if let ast::Expr::Name(ast::ExprName { id, .. }) = arg {
                        arg_names.insert(id.as_str());
                    }
                }
            }
            _ => continue,
        }
        last_expr.replace(expr);
    }

    for f_string in value.f_strings() {
        let mut has_name = false;
        for element in f_string.elements.expressions() {
            if let ast::Expr::Name(ast::ExprName { id, .. }) = element.expression.as_ref() {
                if arg_names.contains(id.as_str()) {
                    return false;
                }
                if semantic
                    .lookup_symbol(id)
                    .map_or(true, |id| semantic.binding(id).kind.is_builtin())
                {
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
