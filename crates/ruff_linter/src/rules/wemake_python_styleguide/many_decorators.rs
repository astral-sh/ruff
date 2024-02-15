use ruff_python_ast::Decorator;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;


#[violation]
pub struct TooManyDecorators {
    decorators: usize,
    max_decorators: usize,
}

impl Violation for TooManyDecorators {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyDecorators {
            decorators,
            max_decorators,
        } = self;
        format!("Too many decorators: ({decorators} > {max_decorators})")
    }
}

pub(crate) fn too_many_decorators(decorator_list: &[Decorator]) -> Option<Diagnostic> {
    let decorators = decorator_list.len();

    if decorators > 2 {
        Some(Diagnostic::new(TooManyDecorators { decorators, max_decorators: 2 }, TextRange::default()))
    } else { None }
}
