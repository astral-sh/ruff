use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    Arguments, Expr, ExprCall, Keyword, Stmt, StmtExpr, StringLiteral, StringLiteralValue,
};
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;

use super::helpers::should_be_fstring;

/// # What it does
/// Checks for strings that contain f-string syntax but are not f-strings.
///
/// ## Why is this bad?
/// An f-string missing an `f` at the beginning won't format anything, and instead
/// treat the interpolation syntax as literal.
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
pub struct MissingFStringSyntax {
    literal: String,
}

impl AlwaysFixableViolation for MissingFStringSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { literal } = &self;
        format!(r#"`{literal}` may be a formatting string without an `f` prefix."#)
    }

    fn fix_title(&self) -> String {
        "Add an `f` prefix to the formatting string".into()
    }
}

pub(crate) fn missing_fstring_syntax(
    diagnostics: &mut Vec<Diagnostic>,
    value: &StringLiteralValue,
    locator: &Locator,
    semantic: &SemanticModel,
) {
    match semantic.current_statement() {
        Stmt::Expr(StmtExpr { value, .. }) => match value.as_ref() {
            Expr::StringLiteral(_) => return,
            _ => {}
        },
        _ => {}
    }

    let kwargs = get_func_keywords(semantic);

    for literal in value.as_slice() {
        if should_be_fstring(&literal.value, kwargs, semantic) {
            let mut diagnostic = Diagnostic::new(
                MissingFStringSyntax {
                    literal: locator.slice(literal.range).to_string(),
                },
                literal.range,
            );
            diagnostic.set_fix(fix_fstring_syntax(literal, locator));
            diagnostics.push(diagnostic);
        }
    }
}

fn get_func_keywords<'a>(semantic: &'a SemanticModel) -> Option<&'a Vec<Keyword>> {
    for expr in [
        semantic.current_expression_parent(),
        semantic.current_expression_grandparent(),
    ]
    .into_iter()
    .filter_map(|e| e)
    {
        match expr {
            Expr::Call(ExprCall {
                arguments: Arguments { keywords, .. },
                ..
            }) => return Some(keywords),
            _ => continue,
        }
    }
    None
}

fn fix_fstring_syntax(literal: &StringLiteral, locator: &Locator) -> Fix {
    let content = format!(r#"f{}"#, locator.slice(literal.range));
    Fix::unsafe_edit(Edit::replacement(
        content,
        literal.range.start(),
        literal.range.end(),
    ))
}
