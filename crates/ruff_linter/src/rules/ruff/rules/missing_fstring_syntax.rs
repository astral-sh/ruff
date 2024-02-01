use memchr::memchr2_iter;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
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
/// this lint applies the following criteria to the string literal:
/// 1. The string literal is not a standalone expression: it needs to be part of an assignment, function call, and so on.
/// 2. If it's an argument in a function call that takes keyword arguments, no keywords can be used as identifiers within the formatting syntax.
/// 3. Every identifier in a {...} section needs to be a valid, in-scope reference, and the string should have at least one identifier.
/// 4. All format specifiers should be valid.
///
/// ## Example
///
/// ```python
/// name = "Sarah"
/// dayofweek = "Tuesday"
/// msg = "Hello {name}! It is {dayofweek} today!"
/// ```
///
/// It should instead be:
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
        "Add an `f` prefix to the f-string".into()
    }
}

pub(crate) fn missing_fstring_syntax(
    diagnostics: &mut Vec<Diagnostic>,
    string_like: ast::StringLike,
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

    let kwargs = get_func_keywords(semantic);

    let mut check = |source, range: TextRange| {
        if should_be_fstring(source, range, kwargs, locator, semantic) {
            let diagnostic =
                Diagnostic::new(MissingFStringSyntax, range).with_fix(fix_fstring_syntax(range));
            diagnostics.push(diagnostic);
        }
    };

    match string_like {
        ast::StringLike::StringLiteral(string_literal) => {
            for literal in string_literal.value.as_slice() {
                check(&literal.value, literal.range());
            }
        }
        ast::StringLike::BytesLiteral(_) => {}
        ast::StringLike::FStringLiteral(fstring_literal_element) => {
            check(
                &fstring_literal_element.value,
                fstring_literal_element.range(),
            );
        }
    }
}

/// Returns `true` if `source` is valid f-string syntax with qualified, bound variables.
/// `kwargs` should be the keyword arguments that were passed to function if the string literal is also
/// being passed to the same function.
/// If a identifier from `kwargs` is used in `source`'s formatting, this will return `false`,
/// since it's possible the function could be formatting the literal in question.
/// Here's a example case where we don't want to suggest turning a literal into an
/// f-string:
/// ```python
/// def alternative_formatter(string, **kwargs):
///     format(string, **kwargs)
///
/// print(alternative_formatter("{fmt}", fmt = "Hello World"))
/// ```
/// In general, if the literal is passed to a function which is also being
/// passed at least one keyword argument with an identifier that also exists in the literal,
/// that literal should return `false`.
/// ```
pub(super) fn should_be_fstring(
    source: &str,
    range: TextRange,
    kwargs: Option<&[ast::Keyword]>,
    locator: &Locator,
    semantic: &SemanticModel,
) -> bool {
    if !has_brackets(source) {
        return false;
    }

    let Ok(ast::Expr::FString(ast::ExprFString { value, .. })) =
        parse_expression(&format!("f{}", locator.slice(range)))
    else {
        return false;
    };

    let kw_idents: FxHashSet<&str> = kwargs
        .map(|keywords| {
            keywords
                .iter()
                .filter_map(|k| k.arg.as_ref())
                .map(ast::Identifier::as_str)
                .collect()
        })
        .unwrap_or_default();

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

fn get_func_keywords<'a>(semantic: &'a SemanticModel) -> Option<&'a [ast::Keyword]> {
    for expr in [
        semantic.current_expression_parent(),
        semantic.current_expression_grandparent(),
    ]
    .into_iter()
    .flatten()
    {
        match expr {
            ast::Expr::Call(ast::ExprCall {
                arguments: ast::Arguments { keywords, .. },
                ..
            }) => return Some(keywords),
            _ => continue,
        }
    }
    None
}

fn fix_fstring_syntax(range: TextRange) -> Fix {
    Fix::unsafe_edit(Edit::insertion("f".into(), range.start()))
}
