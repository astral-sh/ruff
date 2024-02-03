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
/// 1. The string literal is a standalone expression. For example, a docstring.
/// 2. The literal is part of a function call with keyword arguments that match at least one variable (for example: `format("Message: {value}", value = "Hello World")`)
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

    if should_be_fstring(literal, locator, semantic) {
        let diagnostic = Diagnostic::new(MissingFStringSyntax, literal.range())
            .with_fix(fix_fstring_syntax(literal.range()));
        diagnostics.push(diagnostic);
    }
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

    let Ok(ast::Expr::FString(ast::ExprFString { value, .. })) =
        parse_expression(&format!("f{}", locator.slice(literal.range())))
    else {
        return false;
    };

    let mut kwargs = vec![];
    for expr in semantic.current_expressions() {
        match expr {
            ast::Expr::Call(ast::ExprCall {
                arguments: ast::Arguments { keywords, .. },
                func,
                ..
            }) => {
                if let ast::Expr::Attribute(ast::ExprAttribute { .. }) = func.as_ref() {
                    return false;
                }
                kwargs.extend(keywords.iter());
            }
            _ => continue,
        }
    }

    let kw_idents: FxHashSet<&str> = kwargs
        .iter()
        .filter_map(|k| k.arg.as_ref())
        .map(ast::Identifier::as_str)
        .collect();

    for f_string in value.f_strings() {
        let mut has_name = false;
        for element in f_string
            .elements
            .iter()
            .filter_map(|element| element.as_expression())
        {
            if let ast::Expr::Name(ast::ExprName { id, .. }) = element.expression.as_ref() {
                if kw_idents.contains(id.as_str()) || semantic.lookup_symbol(id).is_none() {
                    return false;
                }
                has_name = true;
            }
            if let Some(spec) = &element.format_spec {
                let spec = locator.slice(spec.range());
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
    memchr2_iter(b'{', b'}', possible_fstring.as_bytes()).count() > 1
}

fn fix_fstring_syntax(range: TextRange) -> Fix {
    Fix::unsafe_edit(Edit::insertion("f".into(), range.start()))
}
