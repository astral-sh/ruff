use ruff_python_ast::{self as ast, Expr, Ranged};
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary parentheses on raised exceptions.
///
/// ## Why is this bad?
/// If an exception is raised without any arguments, parentheses are not
/// required, as the `raise` statement accepts either an exception instance
/// or an exception class (which is then implicitly instantiated).
///
/// Removing the parentheses makes the code more concise.
///
/// ## Example
/// ```python
/// raise TypeError()
/// ```
///
/// Use instead:
/// ```python
/// raise TypeError
/// ```
///
/// ## References
/// - [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
#[violation]
pub struct UnnecessaryParenOnRaiseException;

impl AlwaysAutofixableViolation for UnnecessaryParenOnRaiseException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses on raised exception")
    }

    fn autofix_title(&self) -> String {
        format!("Remove unnecessary parentheses")
    }
}

/// RSE102
pub(crate) fn unnecessary_paren_on_raise_exception(checker: &mut Checker, expr: &Expr) {
    if let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    }) = expr
    {
        if args.is_empty() && keywords.is_empty() {
            // `raise func()` still requires parentheses; only `raise Class()` does not.
            if checker
                .semantic()
                .lookup_attribute(func)
                .map_or(false, |id| {
                    checker.semantic().binding(id).kind.is_function_definition()
                })
            {
                return;
            }

            let range = match_parens(func.end(), checker.locator())
                .expect("Expected call to include parentheses");
            let mut diagnostic = Diagnostic::new(UnnecessaryParenOnRaiseException, range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::deletion(func.end(), range.end())));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Return the range of the first parenthesis pair after a given [`TextSize`].
fn match_parens(start: TextSize, locator: &Locator) -> Option<TextRange> {
    let contents = &locator.contents()[usize::from(start)..];

    let mut fix_start = None;
    let mut fix_end = None;
    let mut count = 0u32;

    for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, start).flatten() {
        match tok {
            Tok::Lpar => {
                if count == 0 {
                    fix_start = Some(range.start());
                }
                count = count.saturating_add(1);
            }
            Tok::Rpar => {
                count = count.saturating_sub(1);
                if count == 0 {
                    fix_end = Some(range.end());
                    break;
                }
            }
            _ => {}
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Some(TextRange::new(start, end)),
        _ => None,
    }
}
