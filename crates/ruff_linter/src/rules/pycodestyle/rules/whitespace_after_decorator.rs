use ruff_diagnostic::{AlwaysFixableViolation, Diagnostic, DiagnosticKind};
use ruff_python_ast::ast::{self, Visitor};
use ruff_python_ast::node::Node;
use ruff_python_ast::source_code::Locator;

#[violation]
pub struct WhitespaceAfterDecorator;

impl AlwaysFixableViolation for WhitespaceAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after decorator")
    }

    fn fix_title(&self) -> String {
        "Remove whitespace after decorator".to_string()
    }
}

pub(crate) fn whitespace_after_decorator(
    node: &ast::Stmt,
    locator: &Locator,
) -> Option<Diagnostic> {
    if let ast::Stmt::FunctionDef { decorators, .. } = node {
        for decorator in decorators {
            let decorator_end = decorator.end();

            if let Some(char_after) = locator.get_char(decorator_end) {
                if char_after.is_whitespace() {
                    return Some(Diagnostic::new(
                        DiagnosticKind::AlwaysFixable(WhitespaceAfterDecorator),
                        decorator_end,
                        "Whitespace after decorator".to_string(),
                    ));
                }
            }
        }
    }
    None
}
