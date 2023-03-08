use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::rules::pycodestyle::helpers::is_ambiguous_name;

/// ## What it does
/// Checks for the use of the characters 'l', 'O', or 'I' as variable names.
///
/// ## Why is this bad?
/// In some fonts, these characters are indistinguishable from the
/// numerals one and zero. When tempted to use 'l', use 'L' instead.
///
/// ## Example
/// ```python
/// l = 0
/// O = 123
/// I = 42
/// except AttributeError as O:
/// with lock as l:
/// global I
/// nonlocal l
/// def foo(l):
/// def foo(l=12):
/// l = foo(l=12)
/// for l in range(10):
/// [l for l in lines if l]
/// lambda l: None
/// lambda a=x[1:5], l: None
/// lambda **l:
/// def f(**l):
/// ```
///
/// Use instead:
/// ```python
/// L = 0
/// o = 123
/// i = 42
/// except AttributeError as o:
/// with lock as L:
/// foo(l=12)
/// foo(l=I)
/// for a in foo(l=12):
/// lambda arg: arg * l
/// lambda a=l[I:5]: None
/// lambda x=a.I: None
/// if l >= 12:
/// ```

#[violation]
pub struct AmbiguousVariableName(pub String);

impl Violation for AmbiguousVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousVariableName(name) = self;
        format!("Ambiguous variable name: `{name}`")
    }
}

/// E741
pub fn ambiguous_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            AmbiguousVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}
